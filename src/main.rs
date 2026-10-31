use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::{env, fs};

use lumo::driver::{self, Failure};

const USAGE: &str = "использование:
  lumo check FILE           проверить типы
  lumo build FILE [-o OUT]  собрать в JavaScript
  lumo run FILE             собрать и запустить в Node";

/// Стек главного потока мал для глубоко вложенных выражений: разбор и вывод типов рекурсивны.
const STACK_SIZE: usize = 64 * 1024 * 1024;

fn main() {
    let worker = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(real_main);
    let code = match worker.map(|h| h.join()) {
        Ok(Ok(code)) => code,
        Ok(Err(_)) => 101, // паника уже напечатана потоком
        Err(e) => {
            eprintln!("не удалось запустить рабочий поток: {e}");
            1
        }
    };
    process::exit(code);
}

fn real_main() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["check", file] => check(file),
        ["build", file] => build(file, &Path::new(file).with_extension("js").to_string_lossy()),
        ["build", file, "-o", out] => build(file, out),
        ["run", file] => run(file),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
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

/// Один и тот же файл (с учётом `./` и символических ссылок, если файлы существуют).
fn same_path(a: &str, b: &str) -> bool {
    if Path::new(a) == Path::new(b) {
        return true;
    }
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

fn build(file: &str, out: &str) -> i32 {
    if same_path(file, out) {
        eprintln!("выходной файл совпадает с входным ({out}): укажи -o");
        return 1;
    }
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
    let path = match create_temp_js(&env::temp_dir(), &js) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("не удалось создать временный файл: {e}");
            return 1;
        }
    };
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

/// Сколько раз пробуем новое имя, если временный файл уже существует.
const TEMP_ATTEMPTS: u32 = 16;

/// Создаёт во `dir` новый временный `.js` с правами 0600 и записывает в него `js`.
fn create_temp_js(dir: &Path, js: &str) -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    create_temp_js_named(dir, js, |attempt| format!("lumo-run-{}-{nanos}-{attempt}.js", process::id()))
}

/// `create_new` (O_EXCL) не открывает существующий путь, в том числе символическую ссылку,
/// поэтому заранее подложенный файл не перезаписывается — берём следующее имя.
fn create_temp_js_named(
    dir: &Path,
    js: &str,
    name: impl Fn(u32) -> String,
) -> std::io::Result<PathBuf> {
    for attempt in 0..TEMP_ATTEMPTS {
        let path = dir.join(name(attempt));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        match opts.open(&path) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(js.as_bytes()) {
                    let _ = fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "не удалось подобрать свободное имя",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lumo-main-test-{}-{tag}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn temp_files_get_distinct_paths() {
        let dir = scratch("distinct");
        let a = create_temp_js(&dir, "a").unwrap();
        let b = create_temp_js(&dir, "b").unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(&a).unwrap(), "a");
        assert_eq!(fs::read_to_string(&b).unwrap(), "b");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_file_is_not_overwritten() {
        let dir = scratch("existing");
        fs::write(dir.join("t-0.js"), "чужой").unwrap();
        let path = create_temp_js_named(&dir, "js", |n| format!("t-{n}.js")).unwrap();
        assert_eq!(path, dir.join("t-1.js"));
        assert_eq!(fs::read_to_string(dir.join("t-0.js")).unwrap(), "чужой");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_not_followed() {
        let dir = scratch("symlink");
        let victim = dir.join("victim");
        fs::write(&victim, "нетронуто").unwrap();
        std::os::unix::fs::symlink(&victim, dir.join("t-0.js")).unwrap();
        let path = create_temp_js_named(&dir, "js", |n| format!("t-{n}.js")).unwrap();
        assert_eq!(path, dir.join("t-1.js"));
        assert_eq!(fs::read_to_string(&victim).unwrap(), "нетронуто");
        // висячая ссылка тоже не должна создавать файл по своей цели
        let target = dir.join("dangling-target");
        std::os::unix::fs::symlink(&target, dir.join("u-0.js")).unwrap();
        let path = create_temp_js_named(&dir, "js", |n| format!("u-{n}.js")).unwrap();
        assert_eq!(path, dir.join("u-1.js"));
        assert!(!target.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn temp_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let path = create_temp_js(&dir, "js").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gives_up_after_all_attempts_taken() {
        let dir = scratch("exhaust");
        let e = create_temp_js_named(&dir, "js", |_| "same.js".to_string());
        assert!(e.is_ok());
        let e = create_temp_js_named(&dir, "js", |_| "same.js".to_string()).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_path_detects_aliases() {
        assert!(same_path("a.js", "a.js"));
        assert!(!same_path("a.lumo", "a.js"));
    }
}
