use std::path::PathBuf;
use std::process::{self, Command};
use std::{env, fs};

fn lumo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lumo"))
}

fn write_tmp(name: &str, src: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lumo-cli-{}", process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, src).unwrap();
    path
}

#[test]
fn run_prints_program_output() {
    let file = write_tmp("ok.lumo", "fn main() {\n  print(\"hi\")\n}\n");
    let out = lumo().args(["run", file.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}

#[test]
fn check_reports_type_errors() {
    let file = write_tmp("bad.lumo", "fn main() {\n  print(1)\n}\n");
    let out = lumo().args(["check", file.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ожидался String, а здесь Int"));
}

#[test]
fn build_writes_javascript() {
    let file = write_tmp("build.lumo", "fn main() {\n  print(\"hi\")\n}\n");
    let js = file.with_file_name("build-out.js");
    let out = lumo()
        .args(["build", file.to_str().unwrap(), "-o", js.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(fs::read_to_string(&js).unwrap().contains("function main$main()"));
}

#[test]
fn missing_file_is_reported() {
    let out = lumo().args(["check", "/nonexistent/x.lumo"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("не удалось прочитать"));
}

#[test]
fn usage_on_bad_arguments() {
    let out = lumo().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("использование"));
}
