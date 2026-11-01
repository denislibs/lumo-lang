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
    let out = lumo()
        .args(["run", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}

#[test]
fn check_reports_type_errors() {
    let file = write_tmp("bad.lumo", "fn main() {\n  print(1)\n}\n");
    let out = lumo()
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
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
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::read_to_string(&js)
        .unwrap()
        .contains("function main$main()"));
}

#[test]
fn missing_file_is_reported() {
    let out = lumo()
        .args(["check", "/nonexistent/x.lumo"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("не удалось прочитать"));
}

#[test]
fn usage_on_bad_arguments() {
    let out = lumo().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("использование"));
}

#[test]
fn build_refuses_to_overwrite_its_input() {
    let file = write_tmp("same.js", "fn main() {\n  print(\"hi\")\n}\n");
    let before = fs::read_to_string(&file).unwrap();
    let out = lumo()
        .args(["build", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("выходной файл совпадает с входным"));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    // то же с явным -o
    let out = lumo()
        .args([
            "build",
            file.to_str().unwrap(),
            "-o",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn comparing_functions_fails_at_runtime() {
    let file = write_tmp(
        "fneq.lumo",
        "fn f(x) { x }\nfn main() {\n  let g = f\n  if g == g { print(\"да\") } else { print(\"нет\") }\n}\n",
    );
    let out = lumo()
        .args(["run", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("функции нельзя сравнивать"));
}

#[test]
fn comparing_lists_of_functions_fails_at_runtime() {
    let file = write_tmp(
        "fn_eq.lumo",
        "fn main() {\n  print(if [fn(x) { x }] == [fn(x) { x }] { \"a\" } else { \"b\" })\n}\n",
    );
    let out = lumo()
        .args(["run", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("функции нельзя сравнивать"));
}

#[test]
fn check_reports_non_exhaustive_match() {
    let file = write_tmp(
        "partial.lumo",
        "fn main() {\n  let n = 2\n  match n > 1 {\n    true -> print(\"да\")\n  }\n}\n",
    );
    let out = lumo()
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("не обработан случай `false`"));
    assert!(stderr.contains("match n > 1"), "{stderr}");
}
