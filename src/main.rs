mod app;
mod capture;
mod pipeline;
mod tui;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "tapper", version, about = "A pipeline debugger for Unix shell commands")]
#[command(after_help = "\
EXAMPLES:
    tapper 'cat access.log | grep POST | sort | uniq -c'
        Debug a pipeline and explore each stage in the TUI

    tapper --no-tui 'ls -la | grep .rs | wc -l'
        Print a visual flow diagram and summary to stdout

    tapper --stats 'find . -name \"*.rs\" | xargs wc -l | sort -n'
        Show only line counts, byte sizes, and timing per stage

    tapper --stage 2 'cat data.csv | cut -d, -f1 | sort -u'
        Print the raw output of stage 2 (0-indexed)")]
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

    // Print the flow diagram
    print_flow_diagram(result);
    println!();

    // Print per-stage details
    let mut prev_lines: Option<usize> = None;
    for stage in &result.stages {
        let duration = format!("{:.2?}", stage.duration);
        let bytes = format_bytes(stage.byte_count);

        let filter_info = if let Some(prev) = prev_lines {
            if prev > 0 && stage.line_count < prev {
                let pct = 100.0 * (1.0 - stage.line_count as f64 / prev as f64);
                format!("  \x1b[33m↓ {:.1}% filtered\x1b[0m", pct)
            } else if prev > 0 && stage.line_count > prev {
                let pct = 100.0 * (stage.line_count as f64 / prev as f64 - 1.0);
                format!("  \x1b[36m↑ +{:.1}% expanded\x1b[0m", pct)
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

fn print_flow_diagram(result: &capture::PipelineResult) {
    // Build box contents: command label + line count summary
    let boxes: Vec<(String, String)> = result
        .stages
        .iter()
        .map(|s| {
            let cmd = &s.stage.command;
            let label = if cmd.len() > 16 {
                format!("{}…", &cmd[..15])
            } else {
                cmd.clone()
            };
            let summary = format_lines_short(s.line_count);
            (label, summary)
        })
        .collect();

    // Compute widths: each box is padded to fit its content
    let widths: Vec<usize> = boxes
        .iter()
        .map(|(label, summary)| label.len().max(summary.len()) + 2) // 1 space padding each side
        .collect();

    // Top borders
    let mut top = String::new();
    let mut mid1 = String::new(); // command label
    let mut mid2 = String::new(); // line summary
    let mut bot = String::new();

    // Arrows between stages showing data change
    let mut arrows: Vec<String> = Vec::new();
    for i in 0..boxes.len() {
        if i > 0 {
            let prev_lines = result.stages[i - 1].line_count;
            let cur_lines = result.stages[i].line_count;
            let arrow = if prev_lines > 0 && cur_lines < prev_lines {
                let pct = 100.0 * (1.0 - cur_lines as f64 / prev_lines as f64);
                format!(" \x1b[33m→\x1b[2m{:.0}%↓\x1b[0m ", pct)
            } else if prev_lines > 0 && cur_lines > prev_lines {
                format!(" \x1b[36m→\x1b[2m↑\x1b[0m ")
            } else {
                " → ".to_string()
            };
            arrows.push(arrow);
        }
    }

    // Arrow connector visual width (for layout alignment, ignoring ANSI codes)
    let arrow_display_widths: Vec<usize> = (0..boxes.len().saturating_sub(1))
        .map(|i| {
            let prev_lines = result.stages[i].line_count;
            let cur_lines = result.stages[i + 1].line_count;
            if prev_lines > 0 && cur_lines < prev_lines {
                let pct = 100.0 * (1.0 - cur_lines as f64 / prev_lines as f64);
                let pct_str = format!("{:.0}%↓", pct);
                // " →{pct_str} " = 3 + pct_str.chars().count()
                3 + pct_str.chars().count()
            } else if prev_lines > 0 && cur_lines > prev_lines {
                " →↑ ".chars().count()
            } else {
                " → ".len()
            }
        })
        .collect();

    for (i, (label, summary)) in boxes.iter().enumerate() {
        let w = widths[i];

        if i > 0 {
            let adw = arrow_display_widths[i - 1];
            let spacer = " ".repeat(adw);
            top.push_str(&spacer);
            mid1.push_str(&arrows[i - 1]);
            mid2.push_str(&spacer);
            bot.push_str(&spacer);
        }

        // Top border
        top.push('┌');
        top.push_str(&"─".repeat(w));
        top.push('┐');

        // Command label, centered
        let pad_total = w.saturating_sub(label.len());
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        mid1.push('│');
        mid1.push_str(&" ".repeat(pad_left));
        mid1.push_str(&format!("\x1b[1m{}\x1b[0m", label));
        mid1.push_str(&" ".repeat(pad_right));
        mid1.push('│');

        // Line summary, centered
        let pad_total = w.saturating_sub(summary.chars().count());
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        mid2.push('│');
        mid2.push_str(&" ".repeat(pad_left));
        mid2.push_str(&format!("\x1b[2m{}\x1b[0m", summary));
        mid2.push_str(&" ".repeat(pad_right));
        mid2.push('│');

        // Bottom border
        bot.push('└');
        bot.push_str(&"─".repeat(w));
        bot.push('┘');
    }

    println!("{}", top);
    println!("{}", mid1);
    println!("{}", mid2);
    println!("{}", bot);
}

fn format_lines_short(count: usize) -> String {
    if count == 1 {
        "1 line".to_string()
    } else {
        format!("{} lines", count)
    }
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
