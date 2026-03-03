mod app;
mod capture;
mod pipeline;
mod tui;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "tapper", version, about = "A pipeline debugger for Unix shell commands")]
struct Cli {
    /// The shell pipeline to debug (e.g. 'cat file | grep pattern | sort')
    pipeline: String,

    /// Print results to stdout instead of launching the TUI
    #[arg(long)]
    no_tui: bool,

    /// Only show output of a specific stage (0-indexed)
    #[arg(long)]
    stage: Option<usize>,

    /// Show only statistics (line counts, byte counts, timing)
    #[arg(long)]
    stats: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let pipeline = pipeline::Pipeline::parse(&cli.pipeline)?;

    if pipeline.stages.is_empty() {
        anyhow::bail!("empty pipeline — nothing to debug");
    }

    let result = capture::execute(&pipeline)?;

    if cli.no_tui || cli.stats || cli.stage.is_some() {
        print_results(&cli, &result);
    } else {
        tui::run(result)?;
    }

    Ok(())
}

fn print_results(cli: &Cli, result: &capture::PipelineResult) {
    if let Some(idx) = cli.stage {
        if idx < result.stages.len() {
            let stage = &result.stages[idx];
            print!("{}", String::from_utf8_lossy(&stage.output));
        } else {
            eprintln!(
                "stage {} does not exist (pipeline has {} stages)",
                idx,
                result.stages.len()
            );
            std::process::exit(1);
        }
        return;
    }

    // Print the summary
    println!(
        "\x1b[1mPipeline:\x1b[0m {}",
        result
            .stages
            .iter()
            .map(|s| s.stage.command.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    );
    println!();

    let mut prev_lines: Option<usize> = None;
    for stage in &result.stages {
        let duration = format!("{:.2?}", stage.duration);
        let bytes = format_bytes(stage.byte_count);

        let filter_info = if let Some(prev) = prev_lines {
            if prev > 0 && stage.line_count < prev {
                let pct = 100.0 * (1.0 - stage.line_count as f64 / prev as f64);
                format!("  \x1b[33m[{:.1}% filtered]\x1b[0m", pct)
            } else if prev > 0 && stage.line_count > prev {
                let pct = 100.0 * (stage.line_count as f64 / prev as f64 - 1.0);
                format!("  \x1b[36m[+{:.1}% expanded]\x1b[0m", pct)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let status_color = if stage.exit_code == Some(0) || stage.exit_code.is_none() {
            "\x1b[32m"
        } else {
            "\x1b[31m"
        };

        println!(
            "{}Stage {}:\x1b[0m \x1b[1m{}\x1b[0m",
            status_color,
            stage.stage.index + 1,
            stage.stage.command
        );

        if cli.stats {
            println!(
                "  → {} lines ({}) in {}{}",
                stage.line_count, bytes, duration, filter_info
            );
        } else {
            println!(
                "  → {} lines ({}) in {}{}",
                stage.line_count, bytes, duration, filter_info
            );
            // Show first few lines of output
            let preview: String = String::from_utf8_lossy(&stage.output)
                .lines()
                .take(5)
                .map(|l| format!("    {}", l))
                .collect::<Vec<_>>()
                .join("\n");
            if !preview.is_empty() {
                println!("\x1b[2m{}\x1b[0m", preview);
                if stage.line_count > 5 {
                    println!(
                        "    \x1b[2m... ({} more lines)\x1b[0m",
                        stage.line_count - 5
                    );
                }
            }
        }
        println!();

        prev_lines = Some(stage.line_count);
    }

    println!("\x1b[1mTotal:\x1b[0m {:.2?}", result.total_duration);
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
