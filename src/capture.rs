use crate::pipeline::{Pipeline, Stage};
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Maximum bytes to capture per stage (10 MB)
const MAX_CAPTURE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug)]
pub struct StageResult {
    pub stage: Stage,
    pub output: Vec<u8>,
    pub line_count: usize,
    pub byte_count: usize,
    pub duration: Duration,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

#[derive(Debug)]
pub struct PipelineResult {
    pub stages: Vec<StageResult>,
    pub total_duration: Duration,
}

/// Execute a pipeline, capturing intermediate output at each stage.
///
/// Each stage is run as a subprocess via `sh -c`. Data is tee'd between
/// stages: the output of stage N feeds into stage N+1 while also being
/// captured into a buffer.
/// Check if stdin is a pipe (data is being piped in).
fn stdin_is_pipe() -> bool {
    unsafe { libc::isatty(std::io::stdin().as_raw_fd()) == 0 }
}

/// Read all available data from stdin (non-blocking-ish).
fn read_stdin_data() -> Option<Vec<u8>> {
    if !stdin_is_pipe() {
        return None;
    }
    let mut buf = Vec::new();
    match std::io::stdin().lock().read_to_end(&mut buf) {
        Ok(0) => None,
        Ok(_) => Some(buf),
        Err(_) => None,
    }
}

pub fn execute(pipeline: &Pipeline) -> Result<PipelineResult> {
    let total_start = Instant::now();
    let mut stage_results = Vec::new();

    // If stdin is a pipe, read it and use as input to the first stage
    let mut prev_output: Option<Vec<u8>> = read_stdin_data();

    for stage in &pipeline.stages {
        let start = Instant::now();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&stage.command);
        cmd.stderr(Stdio::piped());
        cmd.stdout(Stdio::piped());

        if prev_output.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn stage {}: {}", stage.index + 1, stage.command))?;

        // Feed input from previous stage
        if let Some(ref input_data) = prev_output {
            if let Some(mut stdin) = child.stdin.take() {
                let data = input_data.clone();
                std::thread::spawn(move || {
                    let _ = stdin.write_all(&data);
                    // stdin is dropped here, closing the pipe
                });
            }
        }

        // Read stdout
        let mut output = Vec::new();
        let mut total_bytes: usize = 0;
        if let Some(mut stdout) = child.stdout.take() {
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        total_bytes += n;
                        if output.len() < MAX_CAPTURE_BYTES {
                            let remaining = MAX_CAPTURE_BYTES - output.len();
                            output.extend_from_slice(&buf[..n.min(remaining)]);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e).context("reading stdout"),
                }
            }
        }

        // Read stderr
        let mut stderr_buf = Vec::new();
        if let Some(stderr) = child.stderr.take() {
            // Limit stderr capture to 64KB
            stderr.take(65536).read_to_end(&mut stderr_buf)?;
        }

        let status = child.wait()?;
        let duration = start.elapsed();

        let line_count = bytecount::count(&output, b'\n')
            + if !output.is_empty() && !output.ends_with(b"\n") {
                1
            } else {
                0
            };

        stage_results.push(StageResult {
            stage: stage.clone(),
            output: output.clone(),
            line_count,
            byte_count: total_bytes,
            duration,
            exit_code: status.code(),
            stderr: String::from_utf8_lossy(&stderr_buf).to_string(),
        });

        prev_output = Some(output);
    }

    Ok(PipelineResult {
        stages: stage_results,
        total_duration: total_start.elapsed(),
    })
}
