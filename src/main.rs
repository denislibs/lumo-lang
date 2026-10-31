use std::io::IsTerminal;
use std::path::Path;
use std::process::{self, Command};
use std::{env, fs};

use lumo::driver::{self, Failure};

const USAGE: &str = "использование:
  lumo check FILE           проверить типы
  lumo build FILE [-o OUT]  собрать в JavaScript
  lumo run FILE             собрать и запустить в Node";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match args.as_slice() {
        ["check", file] => check(file),
        ["build", file] => build(file, &Path::new(file).with_extension("js").to_string_lossy()),
        ["build", file, "-o", out] => build(file, out),
        ["run", file] => run(file),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    process::exit(code);
}

fn read(file: &str) -> Option<String> {
    match fs::read_to_string(file) {
        Ok(src) => Some(src),
        Err(e) => {
            eprintln!("не удалось прочитать {file}: {e}");
            None
        }
    }
}

fn report(f: &Failure) -> i32 {
    eprint!("{}", f.render(std::io::stderr().is_terminal()));
    1
}

fn check(file: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    match driver::check(file, &src) {
        Ok(_) => 0,
        Err(f) => report(&f),
    }
}

fn build(file: &str, out: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    match driver::compile(file, &src) {
        Ok(js) => match fs::write(out, js) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("не удалось записать {out}: {e}");
                1
            }
        },
        Err(f) => report(&f),
    }
}

fn run(file: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    let js = match driver::compile(file, &src) {
        Ok(js) => js,
        Err(f) => return report(&f),
    };
    let path = env::temp_dir().join(format!("lumo-run-{}.js", process::id()));
    if let Err(e) = fs::write(&path, js) {
        eprintln!("не удалось записать {}: {e}", path.display());
        return 1;
    }
    let status = Command::new("node").arg(&path).status();
    let _ = fs::remove_file(&path);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("для lumo run нужен Node.js 18+ в PATH");
            1
        }
        Err(e) => {
            eprintln!("не удалось запустить node: {e}");
            1
        }
    }
}
