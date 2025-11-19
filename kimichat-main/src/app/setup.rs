use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::path::PathBuf;

use crate::cli::Cli;
use crate::config::{ClientConfig, BackendType};
use crate::config::helpers::get_model_config_from_env;
use kimichat_policy::PolicyManager;
use kimichat_llm_api::config::{parse_model_attings, GROQ_API_URL, ANTHROPIC_API_URL, OPENAI_API_URL, get_default_url_for_backend};

/// Application configuration derived from CLI arguments and environment
pub struct AppConfig {
    pub client_config: ClientConfig,
    pub policy_manager: PolicyManager,
    pub work_dir: PathBuf,
    pub api_key: String,
}

/// Set up application configuration from CLI arguments
pub fn setup_from_cli(cli: &Cli) -> Result<AppConfig> {
    // Read KIMICHAT_* environment variables for each model
    let (blu_backend_env, blu_url_env, blu_key_env, blu_model_env) = get_model_config_from_env("blu");
    let (grn_backend_env, grn_url_env, grn_key_env, grn_model_env) = get_model_config_from_env("grn");
    let (red_backend_env, red_url_env, red_key_env, red_model_env) = get_model_config_from_env("red");

    // Precedence: CLI flags > KIMICHAT_* env > legacy env > defaults

    // Blue model configuration
    let backend_blu_model = cli.blu_backend.as_ref()
        .and_then(|s| BackendType::from_str(s))
        .or(blu_backend_env);

    let api_url_blu_model = cli.api_url_blu_model.clone()
        .or(blu_url_env)
        .or_else(|| cli.llama_cpp_url.clone())
        .or_else(|| env::var("ANTHROPIC_BASE_URL_BLU").ok())
        .or_else(|| env::var("ANTHROPIC_BASE_URL").ok());

    let api_key_blu_model = cli.blu_key.clone()
        .or(blu_key_env)
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN_BLU").ok())
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok());

    // Green model configuration
    let backend_grn_model = cli.grn_backend.as_ref()
        .and_then(|s| BackendType::from_str(s))
        .or(grn_backend_env);

    let api_url_grn_model = cli.api_url_grn_model.clone()
        .or(grn_url_env)
        .or_else(|| cli.llama_cpp_url.clone())
        .or_else(|| env::var("ANTHROPIC_BASE_URL_GRN").ok())
        .or_else(|| env::var("ANTHROPIC_BASE_URL").ok());

    let api_key_grn_model = cli.grn_key.clone()
        .or(grn_key_env)
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN_GRN").ok())
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok());

    // Red model configuration
    let backend_red_model = cli.red_backend.as_ref()
        .and_then(|s| BackendType::from_str(s))
        .or(red_backend_env);

    let api_url_red_model = cli.api_url_red_model.clone()
        .or(red_url_env)
        .or_else(|| cli.llama_cpp_url.clone())
        .or_else(|| env::var("ANTHROPIC_BASE_URL_RED").ok())
        .or_else(|| env::var("ANTHROPIC_BASE_URL").ok());

    let api_key_red_model = cli.red_key.clone()
        .or(red_key_env)
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN_RED").ok())
        .or_else(|| env::var("ANTHROPIC_AUTH_TOKEN").ok());

    // Detect backend type (explicit backend or auto-detect from URL)
    let is_anthropic_blu = backend_blu_model.as_ref() == Some(&BackendType::Anthropic)
        || api_url_blu_model.as_ref().map(|url| url.contains("anthropic")).unwrap_or(false);
    let is_anthropic_grn = backend_grn_model.as_ref() == Some(&BackendType::Anthropic)
        || api_url_grn_model.as_ref().map(|url| url.contains("anthropic")).unwrap_or(false);
    let is_anthropic_red = backend_red_model.as_ref() == Some(&BackendType::Anthropic)
        || api_url_red_model.as_ref().map(|url| url.contains("anthropic")).unwrap_or(false);

    // Model name configuration with precedence
    let model_blu_override = cli.model_blu_model.clone()
        .or(blu_model_env)
        .or_else(|| cli.model.clone())
        .or_else(|| {
            if is_anthropic_blu {
                env::var("ANTHROPIC_MODEL_BLU").ok()
                    .or_else(|| env::var("ANTHROPIC_MODEL").ok())
                    .or(Some("claude-3-5-sonnet-20241022".to_string()))
            } else {
                None
            }
        });

    let model_grn_override = cli.model_grn_model.clone()
        .or(grn_model_env)
        .or_else(|| cli.model.clone())
        .or_else(|| {
            if is_anthropic_grn {
                env::var("ANTHROPIC_MODEL_GRN").ok()
                    .or_else(|| env::var("ANTHROPIC_MODEL").ok())
                    .or(Some("claude-3-5-sonnet-20241022".to_string()))
            } else {
                None
            }
        });

    let model_red_override = cli.model_red_model.clone()
        .or(red_model_env)
        .or_else(|| cli.model.clone())
        .or_else(|| {
            if is_anthropic_red {
                env::var("ANTHROPIC_MODEL_RED").ok()
                    .or_else(|| env::var("ANTHROPIC_MODEL").ok())
                    .or(Some("claude-3-5-sonnet-20241022".to_string()))
            } else {
                None
            }
        });

    // Parse model@backend(url) format if present - this has the highest precedence
    // and should override all other model configuration
    let (mut model_blu_override_final, mut model_grn_override_final, mut model_red_override_final) = 
        (model_blu_override, model_grn_override, model_red_override);
    let (mut backend_blu_model_final, mut backend_grn_model_final, mut backend_red_model_final) = 
        (backend_blu_model, backend_grn_model, backend_red_model);
    let (mut api_url_blu_model_final, mut api_url_grn_model_final, mut api_url_red_model_final) = 
        (api_url_blu_model, api_url_grn_model, api_url_red_model);

    if let Some(model_config) = &cli.model {
        // Check if this is the model@backend(url) format
        if model_config.contains('@') {
            let (parsed_model, parsed_backend, parsed_url) = parse_model_attings(model_config);
            
            eprintln!("{} Parsed model configuration: model='{}', backend={:?}, url={:?}", 
                     "🔧".cyan(), parsed_model, parsed_backend, parsed_url);
            
            // Apply the parsed configuration to all models since this was a global --model flag
            model_blu_override_final = Some(parsed_model.clone());
            model_grn_override_final = Some(parsed_model.clone());
            model_red_override_final = Some(parsed_model);
            
            if let Some(ref backend) = parsed_backend {
                backend_blu_model_final = Some(backend.clone());
                backend_grn_model_final = Some(backend.clone());
                backend_red_model_final = Some(backend.clone());
            }
            
            // Use the parsed URL, or if no URL was provided but we have a backend, use the default URL for that backend
            let final_url = parsed_url.or_else(|| {
                if let Some(ref backend) = parsed_backend {
                    get_default_url_for_backend(backend)
                } else {
                    None
                }
            });
            
            if let Some(ref url) = final_url {
                api_url_blu_model_final = Some(url.clone());
                api_url_grn_model_final = Some(url.clone());
                api_url_red_model_final = Some(url.clone());
            }
        }
    }

    // API key is only required if at least one model uses Groq (no API URL specified and no per-model key)
    let needs_groq_key = (api_url_blu_model_final.is_none() && api_key_blu_model.is_none())
                      || (api_url_grn_model_final.is_none() && api_key_grn_model.is_none())
                      || (api_url_red_model_final.is_none() && api_key_red_model.is_none());

    let api_key = if needs_groq_key {
        env::var("GROQ_API_KEY")
            .context("GROQ_API_KEY environment variable not set. Use --api-url-blu-model, --api-url-grn-model, and/or --api-url-red-model with ANTHROPIC_AUTH_TOKEN to use other backends.")?
    } else {
        // Using custom backends with per-model keys, no Groq key needed
        String::new()
    };

    // Use current directory as work_dir so the AI can see project files
    // NB: do NOT use the 'workspace' subdirectory as work_dir
    let work_dir = env::current_dir()?;

    // Create client configuration from CLI arguments
    // Priority: specific flags override general --model flag, but model@backend(url) format has highest precedence
    let client_config = ClientConfig {
        api_key: api_key.clone(),
        backend_blu_model: backend_blu_model_final,
        backend_grn_model: backend_grn_model_final,
        backend_red_model: backend_red_model_final,
        api_url_blu_model: api_url_blu_model_final.clone(),
        api_url_grn_model: api_url_grn_model_final.clone(),
        api_url_red_model: api_url_red_model_final.clone(),
        api_key_blu_model,
        api_key_grn_model,
        api_key_red_model,
        model_blu_model_override: model_blu_override_final.clone(),
        model_grn_model_override: model_grn_override_final.clone(),
        model_red_model_override: model_red_override_final.clone(),
    };

    // Inform user about auto-detected Anthropic configuration
    if is_anthropic_blu {
        if let Some(model_name) = model_blu_override_final.as_ref() {
            eprintln!("{} Anthropic detected for blu_model: using model '{}'", "🤖".cyan(), model_name);
        }
    }
    if is_anthropic_grn {
        if let Some(model_name) = model_grn_override_final.as_ref() {
            eprintln!("{} Anthropic detected for grn_model: using model '{}'", "🤖".cyan(), model_name);
        }
    }

    // Create policy manager based on CLI arguments
    let policy_manager = if cli.auto_confirm {
        eprintln!("{} Auto-confirm mode enabled - all actions will be approved automatically", "🚀".green());
        PolicyManager::allow_all()
    } else if cli.policy_file.is_some() || cli.learn_policies {
        let policy_file = cli.policy_file.clone().unwrap_or_else(|| "policies.toml".to_string());
        let policy_path = work_dir.join(&policy_file);
        match PolicyManager::from_file(&policy_path, cli.learn_policies) {
            Ok(pm) => {
                eprintln!("{} Loaded policy file: {}", "📋".cyan(), policy_path.display());
                if cli.learn_policies {
                    eprintln!("{} Policy learning enabled - user decisions will be saved to policy file", "📚".cyan());
                }
                pm
            }
            Err(e) => {
                eprintln!("{} Failed to load policy file: {}", "⚠️".yellow(), e);
                eprintln!("{} Using default policy (ask for confirmation)", "📋".cyan());
                PolicyManager::new()
            }
        }
    } else {
        PolicyManager::new()
    };

    Ok(AppConfig {
        client_config,
        policy_manager,
        work_dir,
        api_key,
    })
}
