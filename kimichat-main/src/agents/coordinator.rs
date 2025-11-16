use crate::agents::agent::{Agent, Task, TaskType, TaskPriority, AgentResult, ExecutionContext};
use crate::agents::agent_factory::AgentFactory;
use crate::agents::agent_config::AgentConfig;
use crate::agents::visibility::{VisibilityManager, ExecutionPhase};
use crate::chat::history::safe_truncate;
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use colored::Colorize;

/// Planning Coordinator - manages task decomposition and agent dispatch
pub struct PlanningCoordinator {
    agent_factory: Arc<AgentFactory>,
    agent_configs: HashMap<String, AgentConfig>,
    task_queue: Arc<RwLock<VecDeque<Task>>>,
    active_agents: Arc<RwLock<HashMap<String, AgentHandle>>>,
    conversation_state: Arc<RwLock<Vec<crate::agents::agent::ChatMessage>>>,
    visibility_manager: Arc<RwLock<VisibilityManager>>,
}

#[derive(Debug, Clone)]
struct AgentHandle {
    name: String,
    task_id: String,
    start_time: std::time::Instant,
}

impl PlanningCoordinator {
    pub fn new(agent_factory: Arc<AgentFactory>) -> Self {
        let session_id = format!("session_{}", chrono::Utc::now().timestamp());
        Self {
            agent_factory,
            agent_configs: HashMap::new(),
            task_queue: Arc::new(RwLock::new(VecDeque::new())),
            active_agents: Arc::new(RwLock::new(HashMap::new())),
            conversation_state: Arc::new(RwLock::new(Vec::new())),
            visibility_manager: Arc::new(RwLock::new(VisibilityManager::new(session_id))),
        }
    }

    /// Get a reference to the visibility manager
    pub fn visibility_manager(&self) -> Arc<RwLock<VisibilityManager>> {
        Arc::clone(&self.visibility_manager)
    }

    /// Load agent configurations from embedded data and filesystem
    /// Embedded configs are loaded first, then filesystem configs can override them
    pub async fn load_agent_configs(&mut self, config_dir: &std::path::Path) -> Result<()> {
        // First, load embedded agent configs (always available)
        let embedded_configs = super::embedded_configs::get_embedded_agent_configs();
        for (agent_name, config_json) in embedded_configs {
            match serde_json::from_str::<AgentConfig>(config_json) {
                Ok(config) => {
                    if let Err(e) = config.validate() {
                        eprintln!("Warning: Invalid embedded config for {}: {}", agent_name, e);
                        continue;
                    }
                    println!("{} Loaded embedded agent configuration: {}", "📋".blue(), agent_name);
                    eprintln!("[DEBUG] Embedded agent '{}' loaded with {} tools: {:?}",
                             config.name, config.tools.len(), config.tools);
                    self.agent_configs.insert(config.name.clone(), config);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse embedded config for {}: {}", agent_name, e);
                }
            }
        }
        println!("{} Loaded {} embedded agent configurations", "✅".green(), self.agent_configs.len());

        // Then, load from filesystem (can override embedded configs)
        if config_dir.exists() {
            let initial_count = self.agent_configs.len();
            let mut entries = tokio::fs::read_dir(config_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let content = tokio::fs::read_to_string(&path).await?;
                    let config: AgentConfig = serde_json::from_str(&content)?;

                    // Validate configuration
                    config.validate()
                        .map_err(|e| anyhow::anyhow!("Invalid config in {}: {}", path.display(), e))?;

                    let is_override = self.agent_configs.contains_key(&config.name);
                    if is_override {
                        println!("{} Overriding embedded agent with filesystem version: {}", "↳".yellow(), config.name);
                    } else {
                        println!("{} Loaded filesystem agent configuration: {}", "📋".blue(), path.display());
                    }
                    eprintln!("[DEBUG] Agent '{}' loaded with {} tools: {:?}",
                             config.name, config.tools.len(), config.tools);

                    self.agent_configs.insert(config.name.clone(), config);
                }
            }

            let filesystem_count = self.agent_configs.len() - initial_count;
            if filesystem_count > 0 {
                println!("{} Loaded {} additional agent configs from filesystem", "✅".green(), filesystem_count);
            }
        } else {
            println!("{} Config directory not found: {} (using embedded configs only)", "ℹ️".blue(), config_dir.display());
        }

        println!("{} Total agent configurations available: {}", "✅".green(), self.agent_configs.len());
        Ok(())
    }

    /// Process a user request and coordinate agent execution
    pub async fn process_user_request(&mut self, request: &str, context: &ExecutionContext) -> Result<AgentResult> {
        println!("{} Processing request: {}", "🤔".yellow(), request);

        // Set initial phase
        {
            let mut vm = self.visibility_manager.write().await;
            vm.set_phase(ExecutionPhase::Planning);
        }

        // 1. Use planner agent to decompose the request
        println!("{} Invoking planner agent to analyze request...", "🧠".cyan());
        let tasks = self.plan_with_agent(request, context).await?;

        // 2. Set agent selection phase
        {
            let mut vm = self.visibility_manager.write().await;
            vm.set_phase(ExecutionPhase::AgentSelection);
        }

        // 3. Add tasks to queue
        {
            let mut queue = self.task_queue.write().await;
            for task in tasks {
                queue.push_back(task);
            }
        }

        // Display initial queue status
        {
            let vm = self.visibility_manager.read().await;
            let queue_size = self.task_queue.read().await.len();
            vm.display_queue_status(queue_size, 0);
        }

        // Set execution phase
        {
            let mut vm = self.visibility_manager.write().await;
            vm.set_phase(ExecutionPhase::TaskExecution);
        }

        // 4. Execute all tasks
        let mut results = Vec::new();
        while !self.task_queue.read().await.is_empty() {
            // Check for cancellation
            if let Some(ref token) = context.cancellation_token {
                if token.is_cancelled() {
                    println!("{}", "Task execution cancelled by user".bright_yellow());
                    return Err(anyhow::anyhow!("Task execution was cancelled by user"));
                }
            }

            let result = self.execute_next_task(context).await?;
            results.push(result);

            // Display active task stack
            {
                let vm = self.visibility_manager.read().await;
                vm.display_task_stack();
            }

            // Add any new tasks generated by this execution
            if let Some(next_tasks) = results.last().and_then(|r| r.next_tasks.clone()) {
                let mut queue = self.task_queue.write().await;
                for task in next_tasks {
                    queue.push_back(task);
                }
            }

            // Display updated queue status
            {
                let vm = self.visibility_manager.read().await;
                let queue_size = self.task_queue.read().await.len();
                let active_count = self.active_agents.read().await.len();
                vm.display_queue_status(queue_size, active_count);
            }
        }

        // Set aggregation phase
        {
            let mut vm = self.visibility_manager.write().await;
            vm.set_phase(ExecutionPhase::ResultAggregation);
        }

        // 5. Synthesize final response
        let final_result = self.synthesize_response(results).await?;

        // Set completed phase and display summary
        {
            let mut vm = self.visibility_manager.write().await;
            vm.set_phase(ExecutionPhase::Completed);
            vm.display_task_hierarchy();
            vm.display_status_summary();
        }

        Ok(final_result)
    }

    /// Use the planner agent to decompose request into tasks
    async fn plan_with_agent(&self, request: &str, context: &ExecutionContext) -> Result<Vec<Task>> {
        // Get planner agent config
        let planner_config = self.agent_configs.get("planner")
            .ok_or_else(|| anyhow::anyhow!("Planner agent not configured"))?;

        // Build dynamic agent list from loaded configurations
        let mut agent_list = String::from("AVAILABLE SPECIALIZED AGENTS:\n");
        for (name, config) in &self.agent_configs {
            // Skip planner itself
            if name == "planner" {
                continue;
            }

            // Build tools list
            let tools_str = if config.tools.is_empty() {
                "no tools".to_string()
            } else {
                config.tools.join(", ")
            };

            agent_list.push_str(&format!("- {}: {} (tools: {})\n",
                name,
                config.description,
                tools_str
            ));
        }

        // Debug: Print the agent list being sent to planner
        eprintln!("[DEBUG] Sending agent list to planner:\n{}", agent_list);

        // Build conversation context summary for planner
        let mut context_summary = String::new();
        if !context.conversation_history.is_empty() {
            context_summary.push_str("\n\nRECENT CONVERSATION CONTEXT:\n");
            context_summary.push_str("The following information has already been discussed/gathered:\n");

            // Include last few conversation messages for context
            let recent_limit = 5; // Last 5 exchanges
            let start_idx = if context.conversation_history.len() > recent_limit {
                context.conversation_history.len() - recent_limit
            } else {
                0
            };

            for msg in &context.conversation_history[start_idx..] {
                let role_prefix = match msg.role.as_str() {
                    "user" => "User: ",
                    "assistant" => "Assistant: ",
                    _ => &format!("{}: ", msg.role),
                };

                // Truncate very long messages
                let content_preview = if msg.content.chars().count() > 300 {
                    format!("{}... [truncated]", safe_truncate(&msg.content, 300))
                } else {
                    msg.content.clone()
                };

                context_summary.push_str(&format!("{}{}\n", role_prefix, content_preview));
            }

            context_summary.push_str("\nIMPORTANT: Use this context to create SPECIFIC tasks that build on what's already known.\n");
            context_summary.push_str("Don't ask agents to re-discover information that's already in the conversation above.\n");
        }

        // Create planner agent
        let planner = self.agent_factory.create_agent(planner_config)?;

        // Create planning task with dynamic agent list and conversation context
        let task_description = format!("{}{}\n\nAnalyze and decompose this request: {}", agent_list, context_summary, request);

        // Debug: Show first 500 chars of what we're sending to planner
        let preview = if task_description.chars().count() > 500 {
            format!("{}... [truncated]", safe_truncate(&task_description, 500))
        } else {
            task_description.clone()
        };
        eprintln!("[DEBUG] Full task description sent to planner:\n{}", preview);

        let plan_task = Task {
            id: format!("plan_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            description: task_description,
            task_type: TaskType::Simple,
            priority: TaskPriority::Critical,
            metadata: HashMap::new(),
        };

        // Execute planner
        let plan_result = planner.execute(plan_task, context).await;

        // Debug: Show planner's raw output
        eprintln!("[DEBUG] Planner output (first 1000 chars):\n{}",
            if plan_result.content.chars().count() > 1000 {
                format!("{}... [truncated]", safe_truncate(&plan_result.content, 1000))
            } else {
                plan_result.content.clone()
            }
        );

        if !plan_result.success {
            // Fallback to simple task if planner fails
            println!("{} Planner failed, creating single task", "⚠️".yellow());
            return Ok(vec![Task {
                id: format!("task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
                description: request.to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::Medium,
                metadata: HashMap::new(),
            }]);
        }

        // Parse planner output (JSON)
        match self.parse_plan_json(&plan_result.content, request) {
            Ok(tasks) => {
                println!("{} Planner created {} task(s)", "✅".green(), tasks.len());
                Ok(tasks)
            }
            Err(e) => {
                println!("{} Failed to parse plan: {}, creating single task", "⚠️".yellow(), e);
                // Fallback to single task
                Ok(vec![Task {
                    id: format!("task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
                    description: request.to_string(),
                    task_type: TaskType::Simple,
                    priority: TaskPriority::Medium,
                    metadata: HashMap::new(),
                }])
            }
        }
    }

    /// Parse planner's JSON output into tasks
    fn parse_plan_json(&self, json_str: &str, original_request: &str) -> Result<Vec<Task>> {
        use serde_json::Value;

        // Extract JSON from response (might have markdown code blocks)
        let json_str = if let Some(start) = json_str.find('{') {
            if let Some(end) = json_str.rfind('}') {
                &json_str[start..=end]
            } else {
                json_str
            }
        } else {
            json_str
        };

        let plan: Value = serde_json::from_str(json_str)?;

        let strategy = plan["strategy"].as_str().unwrap_or("single_task");
        let subtasks = plan["subtasks"].as_array()
            .ok_or_else(|| anyhow::anyhow!("No subtasks array in plan"))?;

        if strategy == "single_task" || subtasks.is_empty() {
            // Single task - extract agent from first subtask
            let agent_name = if let Some(first_subtask) = subtasks.first() {
                first_subtask["agent"].as_str().unwrap_or("system_operator")
            } else {
                "system_operator"
            };

            eprintln!("[DEBUG] Single task mode - agent assigned: {}", agent_name);

            let mut metadata = HashMap::new();
            metadata.insert("assigned_agent".to_string(), agent_name.to_string());
            metadata.insert("depth".to_string(), "0".to_string());

            return Ok(vec![Task {
                id: format!("task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
                description: original_request.to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::Medium,
                metadata,
            }]);
        }

        // Multiple subtasks
        let mut tasks = Vec::new();
        for (idx, subtask) in subtasks.iter().enumerate() {
            let description = subtask["description"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Subtask missing description"))?;
            let agent_name = subtask["agent"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Subtask missing agent assignment"))?;

            eprintln!("[DEBUG] Subtask {} assigned to agent: {}", idx, agent_name);

            let mut metadata = HashMap::new();
            metadata.insert("assigned_agent".to_string(), agent_name.to_string());
            metadata.insert("depth".to_string(), "0".to_string());

            tasks.push(Task {
                id: format!("task_{}_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0), idx),
                description: description.to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::Medium,
                metadata,
            });
        }

        Ok(tasks)
    }

    /// Analyze user request and create initial task
    async fn analyze_request(&self, request: &str) -> Result<Task> {
        let task_id = format!("task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));

        // Simple heuristics for task analysis
        let (task_type, priority) = if request.contains(" and ") || request.contains(" then ") || request.contains(" after ") {
            (TaskType::Complex, TaskPriority::High)
        } else if request.len() > 200 {
            (TaskType::Complex, TaskPriority::Medium)
        } else {
            (TaskType::Simple, TaskPriority::Medium)
        };

        Ok(Task {
            id: task_id,
            description: request.to_string(),
            task_type,
            priority,
            metadata: HashMap::new(),
        })
    }

    /// Decompose complex tasks into subtasks
    async fn decompose_task(&self, task: Task) -> Result<Vec<Task>> {
        match task.task_type {
            TaskType::Simple => Ok(vec![task]),
            TaskType::Complex => self.decompose_complex_task(task).await,
            TaskType::Parallel(subtasks) => Ok(subtasks),
            TaskType::Sequential(subtasks) => Ok(subtasks),
        }
    }

    /// Decompose a complex task into simpler subtasks
    async fn decompose_complex_task(&self, task: Task) -> Result<Vec<Task>> {
        let description = task.description.to_lowercase();
        let mut subtasks = Vec::new();

        // Common patterns for task decomposition
        if description.contains("read") && description.contains("write") {
            // Separate read and write operations
            subtasks.push(Task {
                id: format!("{}_read", task.id),
                description: "Read the relevant files".to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::High,
                metadata: HashMap::new(),
            });

            subtasks.push(Task {
                id: format!("{}_write", task.id),
                description: "Write the updated content".to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::High,
                metadata: HashMap::new(),
            });
        } else if description.contains("search") && description.contains("modify") {
            // Search then modify pattern
            subtasks.push(Task {
                id: format!("{}_search", task.id),
                description: "Search for the relevant code/files".to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::High,
                metadata: HashMap::new(),
            });

            subtasks.push(Task {
                id: format!("{}_modify", task.id),
                description: "Modify the found code/files".to_string(),
                task_type: TaskType::Simple,
                priority: TaskPriority::High,
                metadata: HashMap::new(),
            });
        } else {
            // Default: keep as single task but mark for specialized agent
            subtasks.push(task);
        }

        Ok(subtasks)
    }

    /// Execute the next task in the queue
    async fn execute_next_task(&self, context: &ExecutionContext) -> Result<AgentResult> {
        let task = {
            let mut queue = self.task_queue.write().await;
            queue.pop_front()
                .ok_or_else(|| anyhow::anyhow!("No tasks in queue"))?
        };

        // Calculate task depth from parent relationship
        let task_depth = task.metadata.get("depth")
            .and_then(|d| d.parse::<usize>().ok())
            .unwrap_or(0);
        let parent_task_id = task.metadata.get("parent_id").cloned();

        println!("{} Executing task: {}", "⚡".cyan(), task.description);

        // Find suitable agent for this task
        let agent = self.find_suitable_agent(&task).await?;

        // Register agent and record task start in visibility manager
        {
            let mut vm = self.visibility_manager.write().await;

            // Register agent if not already registered
            if vm.get_agent_tasks(agent.name()).is_empty() {
                vm.register_agent(
                    agent.name().to_string(),
                    agent.capabilities().iter().map(|c| format!("{:?}", c)).collect()
                );
            }

            // Record task start with hierarchy info
            vm.record_task_start_with_parent(
                task.id.clone(),
                agent.name().to_string(),
                task.description.clone(),
                parent_task_id.clone(),
                task_depth,
            );

            // Update parent's subtask count if applicable
            if let Some(parent_id) = &parent_task_id {
                vm.increment_subtask_count(parent_id);
            }
        }

        // Create execution context for this specific task
        let task_context = ExecutionContext {
            workspace_dir: context.workspace_dir.clone(),
            session_id: context.session_id.clone(),
            tool_registry: Arc::clone(&context.tool_registry),
            llm_client: Arc::clone(&context.llm_client),
            conversation_history: context.conversation_history.clone(),
            terminal_manager: context.terminal_manager.clone(),
            skill_registry: context.skill_registry.clone(),
            todo_manager: context.todo_manager.clone(),
            cancellation_token: context.cancellation_token.clone(),
        };

        // Execute task
        let start_time = std::time::Instant::now();
        
        // Debug: Log which agent is being used
        if *self.visibility_manager.read().await.get_current_phase() != ExecutionPhase::Completed {
            eprintln!("[DEBUG] Executing task '{}' with agent '{}' (preferred model: '{}')", 
                      task.description, agent.name(), agent.preferred_model());
        }
        
        let result = agent.execute(task.clone(), &task_context).await;
        let execution_time = start_time.elapsed();

        // Record task completion
        {
            let mut vm = self.visibility_manager.write().await;
            vm.record_task_completion(
                &task.id,
                Some(format!("Completed in {:.2}s", execution_time.as_secs_f64())),
                result.success,
            );
        }

        // Update conversation history
        {
            let mut history = self.conversation_state.write().await;
            history.push(crate::agents::agent::ChatMessage {
                role: "assistant".to_string(),
                content: format!("Agent '{}' completed task: {}", agent.name(), task.description),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning: None,
            });
        }

        println!("{} Task completed by: {}", "✅".green(), agent.name());

        Ok(result)
    }

    /// Find the best agent for a given task
    async fn find_suitable_agent(&self, task: &Task) -> Result<Arc<dyn Agent>> {
        // First check if task has a pre-assigned agent from the planner
        if let Some(assigned_agent) = task.metadata.get("assigned_agent") {
            eprintln!("[DEBUG] Task has assigned_agent: '{}'", assigned_agent);
            if let Some(config) = self.agent_configs.get(assigned_agent) {
                eprintln!("[DEBUG] Found config for '{}' with tools: {:?}", assigned_agent, config.tools);
                let agent = self.agent_factory.create_agent(config)?;
                println!("{} Using planner-assigned agent '{}' for task", "🎯".purple(), assigned_agent);
                return Ok(Arc::from(agent));
            } else {
                println!("{} Planner assigned '{}' but agent not found, searching...", "⚠️".yellow(), assigned_agent);
            }
        }

        // Fallback: search for suitable agent
        for (agent_name, config) in &self.agent_configs {
            // Skip the planner agent for execution tasks
            if agent_name == "planner" {
                continue;
            }

            let agent = self.agent_factory.create_agent(config)?;

            if agent.can_handle(task) {
                println!("{} Selected agent '{}' for task", "🎯".purple(), agent_name);
                return Ok(Arc::from(agent));
            }
        }

        // Final fallback to a general-purpose agent (not planner)
        for (agent_name, config) in &self.agent_configs {
            if agent_name != "planner" {
                let agent = self.agent_factory.create_agent(config)?;
                println!("{} Using fallback agent '{}' for task", "🔄".yellow(), agent_name);
                return Ok(Arc::from(agent));
            }
        }

        Err(anyhow::anyhow!("No suitable agent found for task"))
    }

    /// Synthesize final response from multiple agent results
    async fn synthesize_response(&self, results: Vec<AgentResult>) -> Result<AgentResult> {
        if results.is_empty() {
            return Ok(AgentResult::error(
                "No tasks were executed".to_string(),
                "synthesis".to_string(),
                "coordinator".to_string(),
            ));
        }

        if results.len() == 1 {
            return Ok(results.into_iter().next().unwrap());
        }

        // Combine multiple results
        let mut combined_content = String::new();
        let mut all_success = true;
        let mut total_time = 0u64;

        for result in &results {
            if !result.success {
                all_success = false;
            }
            total_time += result.execution_time;

            combined_content.push_str(&format!(
                "### Result from {}\n\n{}\n\n",
                result.agent_name,
                result.content
            ));
        }

        let final_result = AgentResult {
            success: all_success,
            content: combined_content.trim().to_string(),
            task_id: "synthesis".to_string(),
            agent_name: "coordinator".to_string(),
            execution_time: total_time,
            metadata: HashMap::new(),
            next_tasks: None,
        };

        println!("{} Synthesized response from {} agents", "🎨".blue(), results.len());

        Ok(final_result)
    }

    /// Get current task queue status
    pub async fn get_queue_status(&self) -> (usize, usize) {
        let queue_size = self.task_queue.read().await.len();
        let active_agents = self.active_agents.read().await.len();
        (queue_size, active_agents)
    }
}