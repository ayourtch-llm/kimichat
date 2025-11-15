/// Embedded agent configurations that are compiled into the binary
/// These are always available even if the agents/configs/ directory doesn't exist
use std::collections::HashMap;

/// All agent configs embedded at compile time
pub fn get_embedded_agent_configs() -> HashMap<&'static str, &'static str> {
    let mut configs = HashMap::new();

    // Embed all .json files from agents/configs/ directory
    configs.insert("code_analyzer", include_str!("../../../agents/configs/code_analyzer.json"));
    configs.insert("code_reviewer", include_str!("../../../agents/configs/code_reviewer.json"));
    configs.insert("file_manager", include_str!("../../../agents/configs/file_manager.json"));
    configs.insert("planner", include_str!("../../../agents/configs/planner.json"));
    configs.insert("search_specialist", include_str!("../../../agents/configs/search_specialist.json"));
    configs.insert("system_operator", include_str!("../../../agents/configs/system_operator.json"));
    configs.insert("terminal_specialist", include_str!("../../../agents/configs/terminal_specialist.json"));

    configs
}
