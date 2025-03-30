// src/ollama_client.rs
use anyhow::{anyhow, Context as _, Result};
use ollama_rs::{
    generation::{
        completion::{request::GenerationRequest, GenerationContext, GenerationResponse},
        parameters::{FormatType, KeepAlive, TimeUnit},
    },
    Ollama,
};
use crate::setup::SystemSetup; // Keep for OS info
// Add imports for file reading and paths
use std::fs;
use std::path::PathBuf;

// Define the prompt filename as a constant
const SYSTEM_PROMPT_FILENAME: &str = "system_prompt.txt";

#[derive(Clone, Debug)]
pub struct OllamaClient {
    client: Ollama,
    model: String,
    host: String,
    // Add field to store the path to the config directory
    config_dir: PathBuf,
}

impl OllamaClient {
    // Update constructor to accept config directory path
    pub fn new(host: &str, model: &str, config_dir: PathBuf) -> Self {
        let ollama_client = Ollama::new(host.to_string(), 11434);
        OllamaClient {
            client: ollama_client,
            model: model.to_string(),
            host: host.to_string(),
            config_dir, // Store the config directory path
        }
    }

    pub async fn generate(
        &self,
        prompt: &str, // Contains OS info + query + history
        context: Option<GenerationContext>,
        system_setup: &SystemSetup, // Still needed for OS info
    ) -> Result<(String, Option<GenerationContext>)> {
        // --- Load System Prompt from File ---
        let system_prompt_path = self.config_dir.join(SYSTEM_PROMPT_FILENAME);
        let system_prompt_template = fs::read_to_string(&system_prompt_path).context(format!(
            "Failed to read system prompt file at: {}",
            system_prompt_path.display()
        ))?;
        // --- End Load System Prompt ---

        // Inject OS into the loaded prompt template
        let os_string = system_setup.platform.to_string();
        let system_prompt = system_prompt_template.replace("{OS}", &os_string);

        // Build the request using the loaded system prompt
        let mut request = GenerationRequest::new(self.model.clone(), prompt.to_string())
            .system(system_prompt) // Use loaded and formatted prompt
            .keep_alive(KeepAlive::Until {
                time: 5,
                unit: TimeUnit::Minutes,
            })
            .format(FormatType::Json);

        if let Some(ctx) = context {
            request = request.context(ctx);
        }

        let response: GenerationResponse = self.client.generate(request).await.map_err(|e| {
            anyhow!(
                "Ollama API error: {}. Verify model '{}' exists and API at {} is reachable",
                e,
                self.model,
                self.host
            )
        })?;

        let cleaned_response = response.response.trim().to_string();
        let new_context = response.context;

        Ok((cleaned_response, new_context))
    }
}