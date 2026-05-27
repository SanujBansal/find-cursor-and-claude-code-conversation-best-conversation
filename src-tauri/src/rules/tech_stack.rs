use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// A high-level, human-friendly description of the technologies used in a
/// project directory. Used by the LLM rubric to know which patterns are
/// worth grading the rule files against.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechStack {
    /// Detected primary languages (e.g. "TypeScript", "Rust").
    pub languages: Vec<String>,
    /// Detected frameworks (e.g. "Next.js", "Tauri", "Tailwind CSS").
    pub frameworks: Vec<String>,
    /// Detected build / dependency tooling (e.g. "npm", "cargo", "uv").
    pub tooling: Vec<String>,
    /// Raw signal files discovered during detection (relative paths).
    pub signal_files: Vec<String>,
    /// `true` when at least one signal file was found.
    pub detected: bool,
}

impl TechStack {
    fn add_language(&mut self, value: &str) {
        push_unique(&mut self.languages, value);
    }

    fn add_framework(&mut self, value: &str) {
        push_unique(&mut self.frameworks, value);
    }

    fn add_tool(&mut self, value: &str) {
        push_unique(&mut self.tooling, value);
    }

    fn add_signal(&mut self, value: &str) {
        push_unique(&mut self.signal_files, value);
    }
}

fn push_unique(vec: &mut Vec<String>, value: &str) {
    if !vec.iter().any(|v| v == value) {
        vec.push(value.to_string());
    }
}

/// Detect the tech stack by sniffing common manifest files at the root.
/// Best-effort: it never fails, it just returns whatever it could detect.
pub fn detect_tech_stack(root: &Path) -> TechStack {
    let mut stack = TechStack::default();
    let mut signals: BTreeSet<String> = BTreeSet::new();

    detect_node(root, &mut stack, &mut signals);
    detect_rust(root, &mut stack, &mut signals);
    detect_python(root, &mut stack, &mut signals);
    detect_go(root, &mut stack, &mut signals);
    detect_ruby(root, &mut stack, &mut signals);
    detect_jvm(root, &mut stack, &mut signals);
    detect_dotnet(root, &mut stack, &mut signals);
    detect_php(root, &mut stack, &mut signals);
    detect_flutter(root, &mut stack, &mut signals);
    detect_swift(root, &mut stack, &mut signals);
    detect_misc(root, &mut stack, &mut signals);

    for s in signals {
        stack.add_signal(&s);
    }
    stack.detected = !stack.languages.is_empty()
        || !stack.frameworks.is_empty()
        || !stack.tooling.is_empty();
    stack
}

fn detect_node(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let pkg = root.join("package.json");
    if !pkg.is_file() {
        return;
    }
    signals.insert("package.json".to_string());
    stack.add_tool("npm");

    let Some(json) = read_json(&pkg) else {
        return;
    };

    let has_ts = root.join("tsconfig.json").is_file()
        || has_dep(&json, "typescript")
        || has_dep(&json, "@types/node");
    if has_ts {
        stack.add_language("TypeScript");
        signals.insert("tsconfig.json".to_string());
    }
    stack.add_language("JavaScript");

    if has_dep(&json, "next") {
        stack.add_framework("Next.js");
    }
    if has_dep(&json, "react") || has_dep(&json, "react-dom") {
        stack.add_framework("React");
    }
    if has_dep(&json, "vue") {
        stack.add_framework("Vue");
    }
    if has_dep(&json, "svelte") || has_dep(&json, "@sveltejs/kit") {
        stack.add_framework("Svelte");
    }
    if has_dep(&json, "@angular/core") {
        stack.add_framework("Angular");
    }
    if has_dep(&json, "express") {
        stack.add_framework("Express");
    }
    if has_dep(&json, "nestjs") || has_dep(&json, "@nestjs/core") {
        stack.add_framework("NestJS");
    }
    if has_dep(&json, "fastify") {
        stack.add_framework("Fastify");
    }
    if has_dep(&json, "tailwindcss") || has_dep(&json, "@tailwindcss/postcss") {
        stack.add_framework("Tailwind CSS");
    }
    if has_dep(&json, "@tauri-apps/api") || has_dep(&json, "@tauri-apps/cli") {
        stack.add_framework("Tauri");
    }
    if has_dep(&json, "electron") {
        stack.add_framework("Electron");
    }
    if has_dep(&json, "remix") || has_dep(&json, "@remix-run/react") {
        stack.add_framework("Remix");
    }
    if has_dep(&json, "astro") {
        stack.add_framework("Astro");
    }
    if has_dep(&json, "prisma") || has_dep(&json, "@prisma/client") {
        stack.add_framework("Prisma");
    }
    if has_dep(&json, "vitest") {
        stack.add_tool("vitest");
    }
    if has_dep(&json, "jest") {
        stack.add_tool("jest");
    }
    if has_dep(&json, "playwright") {
        stack.add_tool("playwright");
    }
    if has_dep(&json, "eslint") {
        stack.add_tool("eslint");
    }

    if root.join("pnpm-lock.yaml").is_file() {
        stack.add_tool("pnpm");
        signals.insert("pnpm-lock.yaml".to_string());
    } else if root.join("yarn.lock").is_file() {
        stack.add_tool("yarn");
        signals.insert("yarn.lock".to_string());
    } else if root.join("bun.lockb").is_file() {
        stack.add_tool("bun");
        signals.insert("bun.lockb".to_string());
    }
}

fn detect_rust(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    // Look at root + any first-level subdir (e.g. src-tauri/Cargo.toml).
    let mut found = false;
    for candidate in cargo_candidates(root) {
        if candidate.is_file() {
            found = true;
            let rel = relative(&candidate, root);
            signals.insert(rel);
            if let Some(body) = read_text(&candidate) {
                if body.contains("tauri =") || body.contains("tauri-build") {
                    stack.add_framework("Tauri");
                }
                if body.contains("axum") {
                    stack.add_framework("Axum");
                }
                if body.contains("actix-web") {
                    stack.add_framework("Actix Web");
                }
                if body.contains("rocket") {
                    stack.add_framework("Rocket");
                }
                if body.contains("tokio") {
                    stack.add_framework("Tokio");
                }
            }
        }
    }
    if found {
        stack.add_language("Rust");
        stack.add_tool("cargo");
    }
}

fn cargo_candidates(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.join("Cargo.toml")];
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                paths.push(entry.path().join("Cargo.toml"));
            }
        }
    }
    paths
}

fn detect_python(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let manifests = [
        "pyproject.toml",
        "requirements.txt",
        "Pipfile",
        "setup.py",
        "setup.cfg",
        "uv.lock",
    ];
    let mut found = false;
    for m in manifests {
        let path = root.join(m);
        if path.is_file() {
            found = true;
            signals.insert(m.to_string());
        }
    }
    if !found {
        return;
    }
    stack.add_language("Python");

    if root.join("uv.lock").is_file() {
        stack.add_tool("uv");
    }
    if root.join("Pipfile").is_file() {
        stack.add_tool("pipenv");
    }
    if root.join("poetry.lock").is_file() {
        stack.add_tool("poetry");
        signals.insert("poetry.lock".to_string());
    }
    if !stack.tooling.iter().any(|t| t == "uv" || t == "pipenv" || t == "poetry") {
        stack.add_tool("pip");
    }

    let body = std::fs::read_to_string(root.join("pyproject.toml")).unwrap_or_default()
        + &std::fs::read_to_string(root.join("requirements.txt")).unwrap_or_default();
    for (needle, label) in [
        ("django", "Django"),
        ("flask", "Flask"),
        ("fastapi", "FastAPI"),
        ("starlette", "Starlette"),
        ("pytorch", "PyTorch"),
        ("torch", "PyTorch"),
        ("tensorflow", "TensorFlow"),
        ("langchain", "LangChain"),
        ("pydantic", "Pydantic"),
    ] {
        if body.to_lowercase().contains(needle) {
            stack.add_framework(label);
        }
    }
}

fn detect_go(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let gomod = root.join("go.mod");
    if !gomod.is_file() {
        return;
    }
    signals.insert("go.mod".to_string());
    stack.add_language("Go");
    stack.add_tool("go modules");

    if let Some(body) = read_text(&gomod) {
        if body.contains("gin-gonic/gin") {
            stack.add_framework("Gin");
        }
        if body.contains("labstack/echo") {
            stack.add_framework("Echo");
        }
        if body.contains("gofiber/fiber") {
            stack.add_framework("Fiber");
        }
    }
}

fn detect_ruby(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let gemfile = root.join("Gemfile");
    if !gemfile.is_file() {
        return;
    }
    signals.insert("Gemfile".to_string());
    stack.add_language("Ruby");
    stack.add_tool("bundler");
    if let Some(body) = read_text(&gemfile) {
        if body.contains("rails") {
            stack.add_framework("Ruby on Rails");
        }
        if body.contains("sinatra") {
            stack.add_framework("Sinatra");
        }
    }
}

fn detect_jvm(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let pom = root.join("pom.xml");
    let gradle = root.join("build.gradle");
    let gradle_kts = root.join("build.gradle.kts");

    if pom.is_file() {
        signals.insert("pom.xml".to_string());
        stack.add_language("Java");
        stack.add_tool("maven");
    }
    if gradle.is_file() {
        signals.insert("build.gradle".to_string());
        stack.add_language("Java");
        stack.add_tool("gradle");
    }
    if gradle_kts.is_file() {
        signals.insert("build.gradle.kts".to_string());
        stack.add_language("Kotlin");
        stack.add_tool("gradle");
    }

    for p in [&pom, &gradle, &gradle_kts] {
        if let Some(body) = read_text(p) {
            if body.contains("spring-boot") || body.contains("springframework") {
                stack.add_framework("Spring Boot");
            }
            if body.contains("quarkus") {
                stack.add_framework("Quarkus");
            }
            if body.contains("ktor") {
                stack.add_framework("Ktor");
            }
        }
    }
}

fn detect_dotnet(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("csproj") || ext.eq_ignore_ascii_case("sln") {
                    signals.insert(relative(&path, root));
                    stack.add_language("C#");
                    stack.add_tool("dotnet");
                }
                if ext.eq_ignore_ascii_case("fsproj") {
                    stack.add_language("F#");
                    stack.add_tool("dotnet");
                }
            }
        }
    }
}

fn detect_php(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let composer = root.join("composer.json");
    if !composer.is_file() {
        return;
    }
    signals.insert("composer.json".to_string());
    stack.add_language("PHP");
    stack.add_tool("composer");
    if let Some(body) = read_text(&composer) {
        if body.contains("laravel/framework") {
            stack.add_framework("Laravel");
        }
        if body.contains("symfony/symfony") || body.contains("symfony/framework-bundle") {
            stack.add_framework("Symfony");
        }
    }
}

fn detect_flutter(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let pubspec = root.join("pubspec.yaml");
    if !pubspec.is_file() {
        return;
    }
    signals.insert("pubspec.yaml".to_string());
    stack.add_language("Dart");
    stack.add_tool("pub");
    if let Some(body) = read_text(&pubspec) {
        if body.contains("flutter:") || body.contains("flutter_test") {
            stack.add_framework("Flutter");
        }
    }
}

fn detect_swift(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    let pkg = root.join("Package.swift");
    if pkg.is_file() {
        signals.insert("Package.swift".to_string());
        stack.add_language("Swift");
        stack.add_tool("swift package manager");
    }
}

fn detect_misc(root: &Path, stack: &mut TechStack, signals: &mut BTreeSet<String>) {
    if root.join("Dockerfile").is_file() {
        stack.add_tool("Docker");
        signals.insert("Dockerfile".to_string());
    }
    if root.join("docker-compose.yml").is_file() || root.join("docker-compose.yaml").is_file() {
        stack.add_tool("Docker Compose");
    }
    if root.join("terraform").is_dir() || root.join("main.tf").is_file() {
        stack.add_tool("Terraform");
    }
    if root.join(".github").join("workflows").is_dir() {
        stack.add_tool("GitHub Actions");
    }
}

fn has_dep(json: &serde_json::Value, name: &str) -> bool {
    for key in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(map) = json.get(key).and_then(|v| v.as_object()) {
            if map.contains_key(name) {
                return true;
            }
        }
    }
    false
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dir(label: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "vibe-stack-test-{}-{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn detects_nextjs_tauri_stack() {
        let dir = make_dir("nextjs-tauri");
        fs::write(
            dir.join("package.json"),
            r#"{
              "dependencies": {
                "next": "^15.0.0",
                "react": "^18.0.0",
                "@tauri-apps/api": "^2.0.0",
                "tailwindcss": "^4.0.0"
              },
              "devDependencies": {
                "typescript": "^5.0.0"
              }
            }"#,
        )
        .unwrap();
        fs::write(dir.join("tsconfig.json"), "{}").unwrap();

        let stack = detect_tech_stack(&dir);
        assert!(stack.detected);
        assert!(stack.languages.iter().any(|l| l == "TypeScript"));
        assert!(stack.frameworks.iter().any(|f| f == "Next.js"));
        assert!(stack.frameworks.iter().any(|f| f == "React"));
        assert!(stack.frameworks.iter().any(|f| f == "Tauri"));
        assert!(stack.frameworks.iter().any(|f| f == "Tailwind CSS"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_rust_cargo() {
        let dir = make_dir("rust");
        fs::write(
            dir.join("Cargo.toml"),
            r#"[package]
            name = "x"
            edition = "2021"

            [dependencies]
            tauri = "2.0"
            tokio = { version = "1", features = ["full"] }
            "#,
        )
        .unwrap();

        let stack = detect_tech_stack(&dir);
        assert!(stack.languages.iter().any(|l| l == "Rust"));
        assert!(stack.tooling.iter().any(|t| t == "cargo"));
        assert!(stack.frameworks.iter().any(|f| f == "Tauri"));
        assert!(stack.frameworks.iter().any(|f| f == "Tokio"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_python_fastapi() {
        let dir = make_dir("py");
        fs::write(
            dir.join("pyproject.toml"),
            r#"[project]
            dependencies = ["fastapi>=0.110", "pydantic>=2"]
            "#,
        )
        .unwrap();

        let stack = detect_tech_stack(&dir);
        assert!(stack.languages.iter().any(|l| l == "Python"));
        assert!(stack.frameworks.iter().any(|f| f == "FastAPI"));
        assert!(stack.frameworks.iter().any(|f| f == "Pydantic"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_dir_returns_undetected_stack() {
        let dir = make_dir("empty");
        let stack = detect_tech_stack(&dir);
        assert!(!stack.detected);
        assert!(stack.languages.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
