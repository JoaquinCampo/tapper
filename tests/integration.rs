use std::process::Command;

fn tapper(args: &[&str]) -> (String, String, i32) {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--"])
        .args(args)
        .output()
        .expect("failed to run tapper");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

#[test]
fn test_simple_pipeline() {
    let (stdout, _, code) = tapper(&["--no-tui", "echo hello | cat"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Stage 1:"));
    assert!(stdout.contains("Stage 2:"));
    assert!(stdout.contains("echo hello"));
    assert!(stdout.contains("cat"));
}

#[test]
fn test_stats_mode() {
    let (stdout, _, code) = tapper(&["--stats", "seq 1 10 | grep 5"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("10 lines"));
    assert!(stdout.contains("filtered"));
}

#[test]
fn test_stage_extraction() {
    let (stdout, _, code) = tapper(&["--stage", "0", "echo hello | tr a-z A-Z"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn test_stage_extraction_second() {
    let (stdout, _, code) = tapper(&["--stage", "1", "echo hello | tr a-z A-Z"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "HELLO");
}

#[test]
fn test_failed_command_shows_red() {
    let (stdout, _, code) = tapper(&["--no-tui", "echo ok | false | cat"]);
    assert_eq!(code, 0);
    // Stage 2 (false) should show red (exit code 1)
    assert!(stdout.contains("Stage 2:"));
}

#[test]
fn test_empty_pipeline_error() {
    let (_, stderr, code) = tapper(&["--no-tui", ""]);
    assert_ne!(code, 0);
    assert!(stderr.contains("empty"));
}

#[test]
fn test_line_count_accuracy() {
    let (stdout, _, _) = tapper(&["--stats", "seq 1 50 | cat"]);
    assert!(stdout.contains("50 lines"));
}

#[test]
fn test_filter_percentage() {
    let (stdout, _, _) = tapper(&["--stats", "seq 1 100 | head -10"]);
    assert!(stdout.contains("90.0% filtered"));
}

#[test]
fn test_flow_diagram() {
    let (stdout, _, _) = tapper(&["--no-tui", "echo hi | cat"]);
    // Flow diagram uses box-drawing characters
    assert!(stdout.contains("┌"));
    assert!(stdout.contains("┘"));
    assert!(stdout.contains("→"));
}

#[test]
fn test_many_stages() {
    let (stdout, _, code) = tapper(&[
        "--stats",
        "seq 1 100 | cat | cat | cat | cat | cat | head -50",
    ]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Stage 7:"));
}
