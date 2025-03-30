// src/command_executor.rs

use crate::setup::SystemSetup;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio, Output as StdOutput}; // Use std::process
use thiserror::Error;
use tokio::process::Command as TokioCommand;
use tokio::task; // Use spawn_blocking

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Command execution failed: {0}")]
    CommandFailure(String),
    #[error("IO error during execution: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Dependency installation failed: {0}")]
    DependencyFailure(String),
    #[error("Unsupported tool on this platform: {0}")]
    UnsupportedPlatform(String),
    #[error("Pipeline execution failed: {0}")]
    PipelineFailure(String),
    #[error("Blocking task failed: {0}")]
    BlockingTaskError(String),
    #[error("Command parsing failed: {0}")] // Added
    CommandParsingError(String),
}


// --- get_tool_from_command function (remains the same) ---
fn get_tool_from_command(command: &str) -> Option<String> {
     command.split_whitespace().next().and_then(|first_part| {
        let path = Path::new(first_part);
        path.file_name().and_then(|os| os.to_str()).map(|s| s.to_string())
           .or_else(|| if first_part.is_empty() { None } else { Some(first_part.to_string()) })
    })
}

// --- NEW: Helper function for basic shell-like argument parsing ---
// Parses a command line, handling simple quoted arguments. Returns (command, args).
fn parse_command_line(line: &str) -> Result<(String, Vec<String>), ExecutionError> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut chars = line.trim().chars().peekable();
    let mut command = None;

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                // Decide whether to include quotes in the arg - usually not
            }
            ' ' | '\t' if !in_quotes => {
                if !current_arg.is_empty() {
                    if command.is_none() {
                        command = Some(current_arg.clone());
                    } else {
                        args.push(current_arg.clone());
                    }
                    current_arg.clear();
                }
            }
            _ => {
                current_arg.push(c);
            }
        }
    }

    if !current_arg.is_empty() {
         if command.is_none() {
            command = Some(current_arg.clone());
         } else {
            args.push(current_arg);
         }
    }

    command.ok_or_else(|| ExecutionError::CommandParsingError("No command found".to_string()))
           .map(|cmd| (cmd, args))
}


// --- execute_command function (Using spawn_blocking with better parsing) ---
pub async fn execute_command(command: &str, setup: &SystemSetup) -> Result<String, ExecutionError> {
    // Tool check remains the same
    let tool_for_check = get_tool_from_command(command).ok_or_else(|| ExecutionError::CommandParsingError("Cannot determine tool from empty command".to_string()))?;
    if cfg!(windows) && ["setoolkit", "msfconsole"].contains(&tool_for_check.as_str()) { return Err(ExecutionError::UnsupportedPlatform(format!("{} requires Linux", tool_for_check))); }
    if let Err(e) = setup.check_and_install_tool(&tool_for_check).await { return Err(ExecutionError::DependencyFailure(e.to_string())); }

    // --- Execute command ---
    let output_result: std::result::Result<StdOutput, ExecutionError> = if cfg!(windows) && command.contains('|') {
        // --- Windows Pipeline Handling via spawn_blocking ---
        println!("Executing Windows pipeline (blocking thread): {}", command);
        let command_clone = command.to_string();

        task::spawn_blocking(move || -> std::io::Result<StdOutput> {
            let parts: Vec<&str> = command_clone.split('|').map(|s| s.trim()).collect();
            let mut children: Vec<std::process::Child> = Vec::new();
            let mut previous_stdout: Option<std::process::ChildStdout> = None;

            for (i, part) in parts.iter().enumerate() {
                 if part.is_empty() { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Empty command part in pipeline")); }

                 // Use the new parser for each part
                 let (cmd_name, cmd_args) = parse_command_line(part)
                     .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?; // Map error to io::Error

                 // *** ADDED LOGGING HERE ***
                 println!("DEBUG: Pipeline Part {}: cmd='{}', args='{:?}'", i + 1, cmd_name, cmd_args);

                 let mut cmd = StdCommand::new(cmd_name);
                 cmd.args(&cmd_args); // Pass parsed args

                 if let Some(stdout) = previous_stdout.take() { cmd.stdin(Stdio::from(stdout)); }
                 else { cmd.stdin(Stdio::inherit()); }

                 // Pipe stdout to next command or capture. Pipe stderr to capture.
                 if i < parts.len() - 1 {
                     cmd.stdout(Stdio::piped());
                     cmd.stderr(Stdio::piped()); // Pipe stderr to potentially capture later if needed
                 } else {
                     // Last command: pipe both stdout and stderr for final capture
                     cmd.stdout(Stdio::piped());
                     cmd.stderr(Stdio::piped());
                 }

                 let mut child = cmd.spawn()?;
                 previous_stdout = child.stdout.take(); // Take stdout for the next potential command

                 if i == parts.len() - 1 {
                     // This is the last command, wait for it and capture its output
                     // *** MODIFIED TO CAPTURE stderr AND status MORE EXPLICITLY ***
                     match child.wait_with_output() {
                         Ok(output) => {
                             // Log status and stderr before returning
                             println!("DEBUG: Final command status: {}", output.status);
                             let stderr_text = String::from_utf8_lossy(&output.stderr);
                             if !stderr_text.is_empty() {
                                 println!("DEBUG: Final command stderr:\n{}", stderr_text);
                             }
                             // Return the captured output
                             return Ok(output);
                         }
                         Err(e) => {
                             println!("DEBUG: Failed to wait_with_output on final command: {}", e);
                             // Return the error
                             return Err(e);
                         }
                     }
                 } else {
                     // Not the last command, store child to wait on later if necessary (though often not needed)
                     children.push(child);
                 }
            }
            // Clean up intermediate children (wait shouldn't block long if they finished/failed)
            for mut child in children { let _ = child.wait(); }
            // This part should ideally not be reached if the loop structure is correct
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Pipeline structure error - loop finished unexpectedly"))
        }).await
        .map_err(|e| ExecutionError::BlockingTaskError(format!("Blocking task failed: {}", e)))
        .and_then(|result| result.map_err(ExecutionError::IoError)) // Maps io::Result<StdOutput> to Result<StdOutput, IoError>

    } else {
        // --- Non-Pipeline / Linux Handling (using TokioCommand) ---
        println!("Executing command via shell: {}", command);
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let arg = if cfg!(windows) { "/C" } else { "-c" };
        TokioCommand::new(shell)
            .arg(arg).arg(command)
            .stdout(Stdio::piped()).stderr(Stdio::piped())
            .output().await.map_err(ExecutionError::IoError) // Maps io::Error to IoError
    };

    // --- Process output (This part remains the same) ---
    match output_result {
        Ok(output) => { // output here is std::process::Output
            if !output.status.success() {
                let stderr_output = String::from_utf8_lossy(&output.stderr);
                let stdout_output = String::from_utf8_lossy(&output.stdout);
                let error_message = if stderr_output.trim().is_empty() { format!("Command failed with status {}. Output:\n{}", output.status, stdout_output) }
                                  else { format!("Command failed with status {}. Error:\n{}", output.status, stderr_output) };
                Err(ExecutionError::CommandFailure(error_message))
            } else { Ok(String::from_utf8_lossy(&output.stdout).into_owned()) }
        }
        Err(e) => Err(e), // Pass through any IoError or BlockingTaskError from above
    }
}