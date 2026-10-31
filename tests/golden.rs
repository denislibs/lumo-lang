//! Сквозные тесты: tests/programs/*.lumo компилируется, запускается в node,
//! stdout сравнивается с *.out.

use std::path::Path;
use std::process::Command;
use std::{env, fs, process};

#[test]
fn golden_programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut programs: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "lumo"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty(), "нет программ в {}", dir.display());

    let mut failures = Vec::new();
    for path in programs {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = fs::read_to_string(&path).unwrap();
        let expected = fs::read_to_string(path.with_extension("out")).unwrap();
        let js = match lumo::driver::compile(&format!("{name}.lumo"), &src) {
            Ok(js) => js,
            Err(f) => {
                failures.push(format!("{name}: ошибка компиляции\n{}", f.render(false)));
                continue;
            }
        };
        let js_path = env::temp_dir().join(format!("lumo-golden-{}-{name}.js", process::id()));
        fs::write(&js_path, js).unwrap();
        let output = Command::new("node").arg(&js_path).output().expect("нужен node в PATH");
        let _ = fs::remove_file(&js_path);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stdout != expected {
            failures.push(format!(
                "{name}:\n--- ожидалось\n{expected}--- получено\n{stdout}--- stderr\n{stderr}"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
