use std::process::Command;

fn main() {
    if std::env::var("SKIP_FRONTEND_BUILD").is_ok() {
        println!("cargo:warning=Frontend-Build skipped.");
        return;
    }

    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(&["/C", "cd frontend && npm install && npm run build"]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(&["-c", "cd frontend && npm install && npm run build"]);
        c
    };

    let status = cmd.status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("frontend build failed with status: {s}"),
        Err(e) => panic!("could not run frontend build: {e}"),
    }
}
