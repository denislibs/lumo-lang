//! Сквозные тесты: tests/programs/*.lumo компилируется, запускается в node,
//! stdout сравнивается с *.out, код возврата должен быть 0, stderr — пустым.

use std::path::Path;
use std::process::Command;
use std::{env, fs, process};

/// Компилирует и запускает программу; `Err` — описание расхождения.
fn run_case(name: &str, src: &str, expected: &str) -> Result<(), String> {
    let js = lumo::driver::compile(&format!("{name}.lumo"), src)
        .map_err(|f| format!("{name}: ошибка компиляции\n{}", f.render(false)))?;
    let js_path = env::temp_dir().join(format!("lumo-golden-{}-{name}.js", process::id()));
    fs::write(&js_path, js).unwrap();
    let output = Command::new("node")
        .arg(&js_path)
        .output()
        .expect("нужен node в PATH");
    let _ = fs::remove_file(&js_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout != expected || !output.status.success() || !stderr.is_empty() {
        return Err(format!(
            "{name}: статус {}\n--- ожидалось\n{expected}--- получено\n{stdout}--- stderr\n{stderr}",
            output.status
        ));
    }
    Ok(())
}

#[test]
fn golden_programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut programs: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "lumo"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty(), "нет программ в {}", dir.display());

    let mut failures = Vec::new();
    for path in programs {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = fs::read_to_string(&path).unwrap();
        let out_path = path.with_extension("out");
        let expected = match fs::read_to_string(&out_path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!(
                    "{name}: нет ожидаемого вывода {}: {e}",
                    out_path.display()
                ));
                continue;
            }
        };
        if let Err(msg) = run_case(&name, &src, &expected) {
            failures.push(msg);
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn harness_rejects_program_that_prints_then_fails() {
    let src =
        "fn f(x) { x }\nfn main() {\n  print(\"до\")\n  let b = f == f\n  print(\"после\")\n}\n";
    let err = run_case("throws", src, "до\nпосле\n").unwrap_err();
    assert!(err.contains("функции нельзя сравнивать"), "{err}");
    // даже если stdout совпал с ожиданием, падение не проходит
    let err = run_case("throws2", src, "до\n").unwrap_err();
    assert!(err.contains("статус"), "{err}");
}
