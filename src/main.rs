use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const DEFAULT_STEAM: &str = r"C:\Program Files (x86)\Steam";

fn default_steam() -> String {
    DEFAULT_STEAM.to_string()
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    profile_id: String,
    #[serde(default = "default_steam")]
    steam_path: String,
    #[serde(default)]
    loaded_profile: Option<usize>,
    profiles: Vec<String>,
}

fn success_log(msg: &str) {
    println!("[Success]:\t{}", msg)
}
fn error_log(msg: &str) {
    eprintln!("[Error]:\t{}", msg)
}
fn comment_log(msg: &str) {
    println!("[Comment]:\t{}", msg)
}

fn prompt(msg: &str) -> Res<String> {
    print!("{}", msg);
    std::io::stdout().flush()?;
    let mut input = String::new();
    let n = std::io::stdin().read_line(&mut input)?;
    if n == 0 {
        return Err("input closed (EOF)".into());
    }
    Ok(input.trim().to_string())
}

fn confirm(msg: &str) -> Res<bool> {
    Ok(prompt(&format!("{} (y/n): ", msg))?.eq_ignore_ascii_case("y"))
}

fn get_home_path(home_var: &str) -> Res<PathBuf> {
    let home = std::env::var_os(home_var)
        .ok_or_else(|| format!("env_var {} not found", home_var))?;
    Ok(PathBuf::from(home))
}

fn guarantee_cfg_path(home: &Path) -> Res<PathBuf> {
    let cfg_path = home.join("Documents").join("configmgr_cfg");
    if !cfg_path.is_dir() {
        comment_log(&format!("creating config dir at \"{}\"", cfg_path.display()));
        fs::create_dir_all(&cfg_path)?;
        success_log("config dir created");
    }
    Ok(cfg_path)
}

fn save_config(file: &Path, cfg: &Config) -> Res<()> {
    let text = serde_json::to_string_pretty(cfg)?;
    fs::write(file, text)?;
    Ok(())
}

fn load_config(file: &Path) -> Res<Config> {
    let text = fs::read_to_string(file)?;
    Ok(serde_json::from_str(&text)?)
}

fn dota_file(cfg: &Config) -> PathBuf {
    Path::new(&cfg.steam_path)
        .join("userdata")
        .join(&cfg.profile_id)
        .join("570")
        .join("remote")
        .join("cfg")
        .join("dotakeys_personal.lst")
}

fn profile_file(cfg_dir: &Path, name: &str) -> PathBuf {
    cfg_dir.join(format!("{}.lst", name))
}

fn valid_name(name: &str) -> Res<()> {
    if name.is_empty() {
        return Err("name is empty".into());
    }
    if name.chars().any(|c| "\\/:*?\"<>|".contains(c)) || name.contains("..") {
        return Err("name contains forbidden characters".into());
    }
    Ok(())
}

fn first_run_dialogue() -> Res<Config> {
    println!("[CONFIG DIALOGUE]");
    let profile_id = prompt("steam profile id (folder name in Steam\\userdata): ")?;
    if profile_id.is_empty() || !profile_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("profile id must be digits only".into());
    }
    let steam = prompt(&format!("steam path (empty = {}): ", DEFAULT_STEAM))?;
    let steam_path = if steam.is_empty() { default_steam() } else { steam };
    Ok(Config {
        profile_id,
        steam_path,
        loaded_profile: None,
        profiles: Vec::new(),
    })
}

fn show(cfg: &Config) {
    if cfg.profiles.is_empty() {
        println!("(no profiles)");
        return;
    }
    for (i, name) in cfg.profiles.iter().enumerate() {
        let mark = if cfg.loaded_profile == Some(i) { " [loaded]" } else { "" };
        println!("{}. {}{}", i + 1, name, mark);
    }
}

fn pick_profile(cfg: &Config, msg: &str) -> Res<usize> {
    if cfg.profiles.is_empty() {
        return Err("no profiles yet".into());
    }
    show(cfg);
    let input = prompt(msg)?;
    let n: usize = input.parse().map_err(|_| "not a number")?;
    if n == 0 || n > cfg.profiles.len() {
        return Err("no profile with this number".into());
    }
    Ok(n - 1)
}

fn cmd_save(cfg: &mut Config, cfg_dir: &Path, config_file: &Path) -> Res<()> {
    let src = dota_file(cfg);
    if !src.is_file() {
        return Err(format!("dota keys file not found: {}", src.display()).into());
    }
    let name = prompt("profile name: ")?;
    valid_name(&name)?;
    let exists = cfg.profiles.contains(&name);
    if exists && !confirm(&format!("profile \"{}\" exists, overwrite?", name))? {
        comment_log("cancelled");
        return Ok(());
    }
    fs::copy(&src, profile_file(cfg_dir, &name))?;
    if !exists {
        cfg.profiles.push(name.clone());
    }
    save_config(config_file, cfg)?;
    success_log(&format!("profile \"{}\" saved", name));
    Ok(())
}

fn cmd_remove(cfg: &mut Config, cfg_dir: &Path, config_file: &Path) -> Res<()> {
    let idx = pick_profile(cfg, "number of profile to remove: ")?;
    let name = cfg.profiles[idx].clone();
    if !confirm(&format!("remove \"{}\"?", name))? {
        comment_log("cancelled");
        return Ok(());
    }
    let file = profile_file(cfg_dir, &name);
    if file.is_file() {
        fs::remove_file(&file)?;
    }
    cfg.profiles.remove(idx);
    cfg.loaded_profile = match cfg.loaded_profile {
        Some(l) if l == idx => None,
        Some(l) if l > idx => Some(l - 1),
        other => other,
    };
    save_config(config_file, cfg)?;
    success_log(&format!("profile \"{}\" removed", name));
    Ok(())
}

fn cmd_swap(cfg: &mut Config, cfg_dir: &Path, config_file: &Path) -> Res<()> {
    let idx = pick_profile(cfg, "number of profile to load: ")?;
    let name = cfg.profiles[idx].clone();
    let src = profile_file(cfg_dir, &name);
    if !src.is_file() {
        return Err(format!("profile file missing: {}", src.display()).into());
    }
    let dst = dota_file(cfg);
    if !confirm("this overwrites current dota binds (unsaved ones are lost), continue?")? {
        comment_log("cancelled");
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dst)?;
    cfg.loaded_profile = Some(idx);
    save_config(config_file, cfg)?;
    success_log(&format!("profile \"{}\" loaded to dota", name));
    Ok(())
}

fn main() {
    let home_path = match get_home_path("USERPROFILE") {
        Ok(p) => p,
        Err(e) => {
            error_log(&e.to_string());
            std::process::exit(1);
        }
    };

    let cfg_dir = match guarantee_cfg_path(&home_path) {
        Ok(p) => p,
        Err(e) => {
            error_log(&format!("dir creation failed: {}", e));
            std::process::exit(1);
        }
    };

    let config_file = cfg_dir.join("config.json");

    let mut config = if config_file.is_file() {
        match load_config(&config_file) {
            Ok(c) => {
                success_log("config loaded");
                c
            }
            Err(e) => {
                error_log(&format!("can't read config: {}", e));
                std::process::exit(1);
            }
        }
    } else {
        let cfg = match first_run_dialogue() {
            Ok(c) => c,
            Err(e) => {
                error_log(&e.to_string());
                std::process::exit(1);
            }
        };
        if let Err(e) = save_config(&config_file, &cfg) {
            error_log(&format!("can't save config: {}", e));
            std::process::exit(1);
        }
        success_log(&format!("config saved to \"{}\"", config_file.display()));
        cfg
    };

    loop {
        println!("\ncommands: save | show | remove | swap | exit");
        let cmd = match prompt("> ") {
            Ok(c) => c,
            Err(e) => {
                error_log(&e.to_string());
                break;
            }
        };
        let result = match cmd.to_lowercase().as_str() {
            "save" => cmd_save(&mut config, &cfg_dir, &config_file),
            "show" => {
                show(&config);
                Ok(())
            }
            "remove" => cmd_remove(&mut config, &cfg_dir, &config_file),
            "swap" => cmd_swap(&mut config, &cfg_dir, &config_file),
            "exit" => break,
            "" => Ok(()),
            _ => Err("unknown command".into()),
        };
        if let Err(e) = result {
            error_log(&e.to_string());
        }
    }
}