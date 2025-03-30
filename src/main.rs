// src/main.rs
mod cli;
mod config;
mod ollama_client;
mod command_executor;
mod core;
mod setup;
mod network;

use anyhow::{Context, Result};
use clap::Parser;
use crate::cli::{Cli, Commands};
use crate::core::AppCore;
use crate::ollama_client::OllamaClient;
use crate::setup::SystemSetup;
use std::path::PathBuf; // Import PathBuf
use std::process::exit;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let setup = setup::SystemSetup::new();

    // --- Config path handling (get directory) ---
    let config_file_path_str: String;
    let config_dir: PathBuf;

    let (is_default, config_path_obj) = if let Some(custom_path) = cli.config.as_ref() {
        (false, custom_path.clone()) // Clone custom path
    } else {
        (true, config::AppConfig::default_path()) // Get default path obj
    };

    config_file_path_str = config_path_obj
        .to_str()
        .context("Config path contains invalid UTF-8")?
        .to_string();

    config_dir = config_path_obj
        .parent()
        .context("Could not determine config directory from path")?
        .to_path_buf(); // Get the parent directory

    // Ensure config directory exists (needed before loading/generating files)
    std::fs::create_dir_all(&config_dir)
        .context(format!("Failed to create config directory: {}", config_dir.display()))?;

    // Generate default config if needed
    if is_default && !config_path_obj.exists() {
        config::AppConfig::generate_default_config()
            .context("Failed to generate default config file")?;
        println!("Created default config at: {}", config_file_path_str);
        // You might also want to generate the default system_prompt.txt here
        // e.g., fs::write(config_dir.join(SYSTEM_PROMPT_FILENAME), DEFAULT_SYSTEM_PROMPT_CONTENT)?;
    }

    // Load config using the string path
    let config = config::AppConfig::from_file(&config_file_path_str)?;
    // --- End config path handling ---


    // --- Ollama setup check (no changes) ---
    if let Err(e) = setup.ensure_ollama().await {
        eprintln!("Ollama setup failed: {}", e);
        if cfg!(windows) {
            eprintln!("On Windows, please install Ollama manually from https://ollama.com");
        }
        exit(1);
    }
    // --- End Ollama setup check ---


    // Ollama client setup (UPDATED)
    let ollama_host = config.ollama_host.as_deref().unwrap_or("http://localhost:11434");
    // Pass the config directory path to the constructor
    let client = ollama_client::OllamaClient::new(
        ollama_host,
        &config.model.name,
        config_dir.clone(), // Pass the determined config directory path
    );


    // --- validate_model function definition ---
    // Needs access to setup, passed as arg
    async fn validate_model(client: &OllamaClient, setup_ref: &SystemSetup) -> Result<()> {
        let test_prompt = "<|im_start|>system\nTest<|im_end|>\n<|im_start|>user\nTest<|im_end|>\n<|im_start|>assistant\n";
        // Pass setup_ref to generate
        let (response, _) = client.generate(test_prompt, None, setup_ref).await?;

        if response.is_empty() {
            anyhow::bail!("Model validation failed. Check:\n1. Model exists (ollama list)\n2. API reachable\n3. Port 11434 accessible");
        }
        Ok(())
    }
    // --- End validate_model function definition ---


    // Call validate_model
    validate_model(&client, &setup).await.context("Model validation failed")?;

    // Application core initialization (client now holds config_dir path if needed later)
    // Note: AppCore::new signature might need update if it now takes the updated client type
    let mut app = AppCore::new(client, setup);


    // --- Command handling (no changes) ---
    match cli.command {
        Commands::Run { query, output } => {
            let response = app.process_query(&query).await?;
            println!("{}", response);
            if let Some(path) = output {
                app.save_output(&response, &path)?;
            }
        }
        Commands::Interactive => {
            todo!("Interactive mode coming soon");
        }
    }
    // --- End Command handling ---

    Ok(())
}