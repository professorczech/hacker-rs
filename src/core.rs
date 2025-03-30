// src/core.rs

use serde::Deserialize;
use serde_json;
use regex::Regex;

use crate::command_executor::{self, ExecutionError};
use crate::ollama_client::OllamaClient;
use crate::setup::SystemSetup;
// Removed unused Context import
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};


// --- ExecutionContext ---
pub struct ExecutionContext {
    pub command_history: Vec<String>,
    pub model_context: Option<ollama_rs::generation::completion::GenerationContext>,
    pub discovered_values: HashMap<String, String>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        ExecutionContext { command_history: Vec::new(), model_context: None, discovered_values: HashMap::new() }
    }
}

// --- Structs for Multi-Step JSON response ---
#[derive(Deserialize, Debug, Clone)]
struct CommandStep {
    step: u32,
    action_type: String,
    command: Option<String>, // Command can be optional now
    purpose: Option<String>,

    // Common Dedicated Fields (Optional)
    #[serde(rename = "PAYLOAD:", default)]
    payload: Option<String>,
    #[serde(rename = "LHOST:", default)]
    lhost: Option<String>,
    #[serde(rename = "RHOST:", default)]
    rhost: Option<String>, // Can also be RHOSTS for multiple targets
    #[serde(rename = "LPORT:", default)]
    lport: Option<String>, // Use String for flexibility
    #[serde(rename = "RPORT:", default)]
    rport: Option<String>, // Use String for flexibility
    #[serde(rename = "EXITFUNC:", default)] // Common payload option
    exitfunc: Option<String>, // e.g., "thread", "process", "seh", "none"
    #[serde(rename = "TARGETURI:", default)] // Common web option
    targeturi: Option<String>,

    // Generic Options Map for everything else
    #[serde(default)] // Use default for the map itself
    options: HashMap<String, String>,
}

#[derive(Deserialize, Debug)]
struct MultiStepResponse {
    explanation: Option<String>,
    #[serde(default)]
    steps: Vec<CommandStep>,
}

// --- AppCore struct ---
pub struct AppCore {
    client: OllamaClient,
    context: ExecutionContext,
    system_setup: SystemSetup,
}

// --- AppCore impl ---
impl AppCore {
    // --- new function ---
    pub fn new(client: OllamaClient, system_setup: SystemSetup) -> Self {
        AppCore { client, context: ExecutionContext::new(), system_setup }
    }

    // --- process_query function ---
    pub async fn process_query(&mut self, query: &str) -> Result<String> {
        self.context.discovered_values.clear();
    
        // *** START: Add pre-parsing logic here ***
        println!("DEBUG: Parsing initial query: '{}'", query);

        // Regex for CIDR subnet (e.g., 192.168.1.0/24) - This one is fine
        let cidr_re = Regex::new(r"\b((?:[0-9]{1,3}\.){3}[0-9]{1,3}/\d{1,2})\b")
                        .expect("Invalid CIDR regex");
        // Regex for single IP - REMOVED the unsupported negative lookahead
        let ip_re = Regex::new(r"\b((?:[0-9]{1,3}\.){3}[0-9]{1,3})\b")
                        .expect("Invalid IP regex");

        // Check for CIDR first
        if let Some(captures) = cidr_re.find(query) {
            let discovered_cidr = captures.as_str().to_string();
            println!(">>> Discovered user-provided subnet_cidr: {}", discovered_cidr);
            // Store with the key the LLM expects for subnets
            self.context.discovered_values.insert("subnet_cidr".to_string(), discovered_cidr);
        } else if let Some(captures) = ip_re.find(query) { // Only look for single IP if CIDR wasn't found
            let discovered_ip = captures.as_str().to_string();
            println!(">>> Discovered user-provided target_ip: {}", discovered_ip);
            // Store with the key the LLM expects for single targets
            self.context.discovered_values.insert("target_ip".to_string(), discovered_ip);
        }
        // Add hostname regex/logic here if needed

        println!("DEBUG: Values *after* query parse: {:?}", self.context.discovered_values);
        // *** END: Corrected pre-parsing logic ***    
    
        println!("\n--- Generating Plan ---");
        // Pass the original query, but discovered_values is now pre-populated
        let prompt = self.build_prompt(query);
    
        let (json_response_str, new_context) = match self.client
            .generate(&prompt, self.context.model_context.clone(), &self.system_setup)
            .await {
            Ok(resp) => resp,
            Err(e) => return Err(e.context("LLM generation failed")),
        };
        self.context.model_context = new_context;

        // Call execute_llm_plan without passing discovered_values explicitly
        match self.execute_llm_plan(&json_response_str).await { // <-- Removed extra argument
            Ok(output_message) => Ok(output_message),
            Err(e) => {
                eprintln!("Error processing plan: {}. Raw response: {}", e, json_response_str);
                Ok(format!("Error during processing: {}. Raw response was:\n{}", e, json_response_str))
            }
        }
    }


    // --- Function to execute the multi-step plan (Signature reverted) ---
    async fn execute_llm_plan(&mut self, json_response: &str) -> Result<String> {
        // *** ADD LOGGING HERE to see the raw response ***
        println!("DEBUG: Raw LLM JSON response:\n>>>\n{}\n<<<", json_response);

        match serde_json::from_str::<MultiStepResponse>(json_response) {
            Ok(plan) => {
                let explanation = plan.explanation.unwrap_or_else(|| "Executing plan...".to_string());
                println!("{}", explanation); // This prints "Executing plan..." the first time

                if plan.steps.is_empty() {
                    println!("INFO: LLM returned empty steps array."); // Add confirmation log
                    // Returns early, wrapping explanation in Ok
                    return Ok(explanation);
                }

                let mut step_outputs = Vec::new();
                let final_explanation = explanation.clone(); // Use cloned explanation for final summary

                for step in &plan.steps {
                    let purpose = step.purpose.as_deref().unwrap_or("N/A").to_lowercase();
                    println!("\n--- Running Step {}: {} ---", step.step, purpose);

                    if step.action_type != "command" {
                         println!("Skipping non-command action type: {}", step.action_type);
                         step_outputs.push(format!("Step {}: Skipped (Action Type: {})", step.step, step.action_type));
                         continue;
                    }

                    // DEBUG print remains helpful for now
                    println!("DEBUG: Values before substitution for Step {}: {:?}", step.step, self.context.discovered_values);

                    // --- Substitute Placeholders ---
                let command_to_run = if let Some(command_template) = &step.command {
                    // If there IS a command template string, substitute placeholders in it
                    match self.substitute_placeholders(command_template.as_str()).await { // Use .as_str() here
                        Ok(cmd) => cmd,
                        Err(e) => return Err(anyhow!("Failed step {}: Substituting placeholders failed: {}", step.step, e)),
                    }
                } else {
                    // If step.command is None, set command_to_run to empty string
                    println!("DEBUG: Step {} has no command string, proceeding with empty command.", step.step);
                    String::new()
                };
                // --- End Substitution ---

                let sanitized_command = sanitize_command(&command_to_run);

                // *** Declare step_output here, before the conditional execution ***
                let mut step_output: String;

                // Decide whether to execute command or skip
                if sanitized_command.is_empty() && step.command.is_none() {
                    println!("INFO: Skipping execution for step {} as command is empty and was not defined.", step.step);
                    // Assign the specific "skipped" message
                    step_output = "Skipped (No command)".to_string(); // <<< Assignment
                } else {
                    // --- Execute Command --- (Only run if sanitized_command is not empty or was originally Some)
                    println!("Executing: {}", sanitized_command);
                    match command_executor::execute_command(&sanitized_command, &self.system_setup).await {
                        Ok(output) => {
                            println!("Output:\n{}", output);
                            step_output = output.clone(); // <<< Assignment
                            // Parse output
                            self.parse_and_store_output(step, &sanitized_command, &step_output);
                        }
                        Err(e) => match e {
                            ExecutionError::UnsupportedPlatform(msg) => {
                                eprintln!("Skipping command (Unsupported Platform): {}", msg);
                                step_output = "Skipped (Unsupported Platform)".to_string(); // <<< Assignment
                            }
                            _ => {
                                // If execution fails for other reasons, we return early,
                                // so step_output doesn't need assignment here for the later code path.
                                eprintln!("Command Execution Failed: {}", e);
                                return Err(anyhow!("Execution failed at step {}: {}", step.step, e));
                            }
                        }
                    }
                    // --- End Command Execution ---
                } // End of the 'else' block for execution

                // Now, step_output is guaranteed to be initialized on all paths that reach here
                self.context.command_history.push(format!("Step {}: {} ->\n{}", step.step, sanitized_command, step_output));
                step_outputs.push(format!("Output from Step {}:\n{}", step.step, step_output));

            } // End loop

            Ok(format!("Plan Execution Summary:\n{}\n\n{}", final_explanation, step_outputs.join("\n---\n")))
            }
            // Error handling remains the same
            Err(e) => Err(anyhow!("Failed to parse LLM JSON plan: {}. Raw response: {}", e, json_response)),
        }
}

    // --- Placeholder substitution helper (Reverted to method on &self) ---
    async fn substitute_placeholders(&self, command_template: &str) -> Result<String> {
        let mut final_command = command_template.to_string();
        let placeholder_re = Regex::new(r"\{([a-zA-Z0-9_]+)\}").expect("Invalid placeholder regex");
        let placeholders: Vec<String> = placeholder_re.captures_iter(command_template).filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())).collect();

        if !placeholders.is_empty() {
            println!("DEBUG: Attempting to substitute placeholders in '{}': {:?}", command_template, placeholders);
        }
        for placeholder_name in placeholders {
            // Access map via self.context
            if let Some(value) = self.context.discovered_values.get(&placeholder_name) {
                println!("DEBUG: Substituting {{{}}} with '{}'", placeholder_name, value);
                let placeholder_tag = format!("{{{}}}", placeholder_name);
                final_command = final_command.replace(&placeholder_tag, value);
            } else {
                 println!("DEBUG: Placeholder {{{}}} not found in discovered values: {:?}", placeholder_name, self.context.discovered_values);
                return Err(anyhow!("Required information '{}' for command not found from previous steps.", placeholder_name));
            }
        }
        Ok(final_command)
    }

     // --- Output parsing and storing helper (Reverted to method on &mut self) ---
     fn parse_and_store_output(&mut self, step: &CommandStep, _command_context: &str, output: &str) {
        let purpose = step.purpose.as_deref().unwrap_or("").to_lowercase();
        // Check if the purpose is STILL finding the gateway, even if the command is just "ipconfig"
        if purpose.contains("find default gateway") || purpose.contains("find router") {
            let gateway_ip = if cfg!(windows) {
                // Keep the same regex
                let re = Regex::new(r"Default Gateway.*: ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)").ok();
                // Search ALL lines of the captured output directly in Rust
                output.lines().find_map(|line| {
                    println!("DEBUG: Checking line: {}", line); // Add verbose debug printing
                    re.as_ref().and_then(|r| r.captures(line)).and_then(|cap| cap.get(1)).map(|m| m.as_str())
                })
            } else { // Linux/macOS logic remains the same
                let re_linux = Regex::new(r"default via ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)").ok();
                let re_macos = Regex::new(r"gateway: ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)").ok();
                re_linux.and_then(|r| r.captures(output)).and_then(|cap| cap.get(1)).map(|m| m.as_str())
                .or_else(|| re_macos.and_then(|r| r.captures(output)).and_then(|cap| cap.get(1)).map(|m| m.as_str()))
            };
    
            if let Some(ip) = gateway_ip {
                // Your existing logic to store the IP...
                if ip != "0.0.0.0" {
                    println!(">>> Discovered default_gateway: {}", ip);
                    self.context.discovered_values.insert("default_gateway".to_string(), ip.to_string());
                    println!("DEBUG: Values *after* insert in parse_and_store_output: {:?}", self.context.discovered_values);
                } else {
                    println!("WARN: Parsed gateway IP was 0.0.0.0, ignoring.");
                }
            } else {
                println!("WARN: Could not parse default gateway from output for step {}. Full output was:\n{}", step.step, output); // Log full output on failure
            }
        }
    }

    // --- build_prompt function ---
    fn build_prompt(&self, query: &str) -> String {
        let os_info = self.system_setup.platform.to_string();
        let history_context = self.context.command_history.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join("\n---\n");
        format!(
            "<|im_start|>user\nOS: {}\nTask: {}\nPrevious Commands/Outputs Context:\n{}\n<|im_end|>\n\
            <|im_start|>assistant\n",
            os_info, query, if history_context.is_empty() { "None" } else { &history_context }
        )
    }

    // --- save_output function ---
     pub fn save_output(&self, output: &str, path: &PathBuf) -> Result<()> {
         let mut file = File::create(path)?;
         file.write_all(output.as_bytes())?;
         Ok(())
     }

} // End impl AppCore

// --- Helper function for sanitization ---
fn sanitize_command(raw_command: &str) -> String {
    // ... (implementation remains the same) ...
     let parts: Vec<&str> = raw_command.split_whitespace().collect();
    if parts.is_empty() { raw_command.to_string() } else {
        let command_part = parts[0];
        if command_part.contains('/') || command_part.contains('\\') {
            let base_name = Path::new(command_part).file_name().and_then(|os| os.to_str()).unwrap_or(command_part);
            let mut reconstructed_parts = vec![base_name];
            reconstructed_parts.extend_from_slice(&parts[1..]);
            reconstructed_parts.join(" ")
        } else { raw_command.to_string() }
    }
}