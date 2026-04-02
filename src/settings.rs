use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveTime;
use clap::{Args, Parser, Subcommand};
use config::{Config, FileFormat, Value};
use pnet::datalink::MacAddr;
use std::collections::HashMap;
use std::ffi::OsString;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const DEFAULT_CONFIG_FILE: &str = include_str!("default_config.yml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Console,
    Service,
}

#[derive(Debug, Clone)]
pub struct RunCommand {
    pub settings: Settings,
    pub debug: bool,
    pub mode: RuntimeMode,
}

#[derive(Debug, Clone)]
pub enum ServiceCommand {
    Install { config: PathBuf },
    Uninstall,
    Start,
    Stop,
}

#[derive(Debug, Clone)]
pub enum Command {
    Run(RunCommand),
    RunService,
    Service(ServiceCommand),
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
    #[command(flatten)]
    run: RunArgs,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    Run(RunArgs),
    #[command(name = "run-service", hide = true)]
    RunService(RunArgs),
    #[command(flatten_help = true)]
    Service {
        #[command(subcommand)]
        command: ServiceCliCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ServiceCliCommand {
    Install(InstallArgs),
    Uninstall,
    Start,
    Stop,
}

#[derive(Args, Debug)]
struct InstallArgs {
    #[arg(short = 'c', long = "config")]
    config: PathBuf,
}

#[derive(Args, Debug, Clone, Default)]
struct RunArgs {
    #[arg(short = 'c', long = "config", default_value = "config.yml")]
    config: PathBuf,
    #[arg(short = 'D', long = "debug", default_value_t = false)]
    debug: bool,
    #[arg(short = 'm', long = "mac")]
    mac: Option<String>,
    #[arg(short = 'i', long = "ip")]
    ip: Option<String>,
    #[arg(short = 'u', long = "username")]
    username: Option<String>,
    #[arg(short = 'p', long = "password")]
    password: Option<String>,
    #[arg(short = 'H', long = "host")]
    host: Option<String>,
    #[arg(short = 'N', long = "hostname")]
    hostname: Option<String>,
    #[arg(short = 't', long = "time")]
    time: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub config_path: PathBuf,
    pub mac: Option<MacAddr>,
    pub ip: Option<IpAddr>,
    pub username: String,
    pub password: String,
    pub dns: Vec<SocketAddr>,
    pub host: String,
    pub hostname: String,
    pub time: NaiveTime,
    pub reconnect: u64,
    pub heartbeat: Heartbeat,
    pub retry: Retry,
    pub data: Data,
    #[cfg(feature = "log4rs")]
    pub log: Log,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            config_path: PathBuf::from("config.yml"),
            mac: None,
            ip: None,
            username: String::new(),
            password: String::new(),
            dns: Vec::new(),
            host: String::from("s.scut.edu.cn"),
            hostname: String::new(),
            time: NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
            reconnect: 15,
            heartbeat: Heartbeat::default(),
            retry: Retry::default(),
            data: Data::default(),
            #[cfg(feature = "log4rs")]
            log: Log::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Heartbeat {
    pub eap_timeout: i32,
    pub udp_timeout: i32,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Heartbeat {
            eap_timeout: 60,
            udp_timeout: 12,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Retry {
    pub count: i32,
    pub interval: i32,
}

impl Default for Retry {
    fn default() -> Self {
        Retry {
            count: 2,
            interval: 5000,
        }
    }
}

#[cfg(feature = "log4rs")]
#[derive(Debug, Clone)]
pub struct Log {
    pub enable_console: bool,
    pub enable_file: bool,
    pub file_directory: String,
    pub level_filter: log::LevelFilter,
}

#[cfg(feature = "log4rs")]
impl Default for Log {
    fn default() -> Self {
        Log {
            enable_console: true,
            enable_file: true,
            file_directory: String::from("./logs"),
            level_filter: log::LevelFilter::Info,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Data {
    pub response_identity: ResponseIdentity,
    pub response_md5_challenge: ResponseMd5Challenge,
    pub misc_info: MiscInfo,
}

#[derive(Debug, Clone)]
pub struct ResponseIdentity {
    pub unknown: Vec<u8>,
}

impl Default for ResponseIdentity {
    fn default() -> Self {
        ResponseIdentity {
            unknown: hex::decode("0044610000").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResponseMd5Challenge {
    pub unknown: Vec<u8>,
}

impl Default for ResponseMd5Challenge {
    fn default() -> Self {
        ResponseMd5Challenge {
            unknown: hex::decode("0044612a00").unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MiscInfo {
    pub unknown1: Vec<u8>,
    pub cks32_param: Vec<u8>,
    pub unknown2: Vec<u8>,
    pub os_major: Vec<u8>,
    pub os_minor: Vec<u8>,
    pub os_build: Vec<u8>,
    pub os_unknown: Vec<u8>,
    pub version: Vec<u8>,
    pub hash: String,
}

impl Default for MiscInfo {
    fn default() -> Self {
        MiscInfo {
            unknown1: hex::decode("0222002a").unwrap(),
            cks32_param: hex::decode("c72f3101").unwrap(),
            unknown2: hex::decode("94000000").unwrap(),
            os_major: hex::decode("06000000").unwrap(),
            os_minor: hex::decode("02000000").unwrap(),
            os_build: hex::decode("f0230000").unwrap(),
            os_unknown: hex::decode("02000000").unwrap(),
            version: hex::decode("4472434f4d0096022a").unwrap(),
            hash: String::from("4eb81fc048a5585b7dfe1783155241a328b103c6"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ParseOptions {
    create_default_config: bool,
}

pub fn parse_cli() -> Result<Command> {
    parse_cli_from(std::env::args_os())
}

pub fn parse_run_service() -> Result<RunCommand> {
    match parse_cli_from(std::env::args_os())? {
        Command::Run(run) => Ok(run),
        Command::RunService => {
            let cli = Cli::parse_from(std::env::args_os());
            match cli.command {
                Some(CliCommand::RunService(args)) => build_run_command(args, RuntimeMode::Service),
                _ => bail!("run-service command was not provided."),
            }
        }
        Command::Service(_) => {
            bail!("Service management command cannot start the service runtime.")
        }
    }
}

pub fn parse_cli_from<I, T>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    match cli.command {
        Some(CliCommand::Run(args)) => {
            Ok(Command::Run(build_run_command(args, RuntimeMode::Console)?))
        }
        Some(CliCommand::RunService(args)) => {
            let _ = build_run_command(args, RuntimeMode::Service)?;
            Ok(Command::RunService)
        }
        Some(CliCommand::Service { command }) => {
            Ok(Command::Service(build_service_command(command)?))
        }
        None => Ok(Command::Run(build_run_command(
            cli.run,
            RuntimeMode::Console,
        )?)),
    }
}

fn build_service_command(command: ServiceCliCommand) -> Result<ServiceCommand> {
    Ok(match command {
        ServiceCliCommand::Install(args) => {
            if !args.config.is_absolute() {
                bail!("Service config path must be absolute.");
            }
            let config = absolute_path(&args.config)?;
            ServiceCommand::Install { config }
        }
        ServiceCliCommand::Uninstall => ServiceCommand::Uninstall,
        ServiceCliCommand::Start => ServiceCommand::Start,
        ServiceCliCommand::Stop => ServiceCommand::Stop,
    })
}

fn build_run_command(args: RunArgs, mode: RuntimeMode) -> Result<RunCommand> {
    let options = ParseOptions {
        create_default_config: matches!(mode, RuntimeMode::Console),
    };
    let mut settings = Settings::parse(&args, options)?;

    #[cfg(feature = "log4rs")]
    if matches!(mode, RuntimeMode::Service) {
        settings.log.enable_console = false;
    }

    Ok(RunCommand {
        settings,
        debug: args.debug,
        mode,
    })
}

fn get_str_from_map(map: &HashMap<String, Value>, key: &str) -> Option<String> {
    map.get(key)?.to_owned().into_string().ok()
}

fn get_int_from_map(map: &HashMap<String, Value>, key: &str) -> Option<i64> {
    map.get(key)?.to_owned().into_int().ok()
}

fn get_bool_from_map(map: &HashMap<String, Value>, key: &str) -> Option<bool> {
    map.get(key)?.to_owned().into_bool().ok()
}

fn get_map_from_map(map: &HashMap<String, Value>, key: &str) -> Option<HashMap<String, Value>> {
    map.get(key)?.to_owned().into_table().ok()
}

impl Settings {
    fn read_config(config_path: &Path, options: ParseOptions) -> Result<Config> {
        if !config_path.is_file() {
            if !options.create_default_config {
                bail!("Config file '{}' does not exist.", config_path.display());
            }
            if let Some(parent) = config_path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("Can't create config directory '{}'.", parent.display())
                })?;
            }
            std::fs::write(config_path, DEFAULT_CONFIG_FILE).with_context(|| {
                format!(
                    "Can't create default config file '{}'.",
                    config_path.display()
                )
            })?;
        }

        Config::builder()
            .add_source(
                config::File::new(config_path.to_string_lossy().as_ref(), FileFormat::Yaml)
                    .required(true),
            )
            .build()
            .with_context(|| format!("Can't read config file '{}'.", config_path.display()))
    }

    fn resolve(mut settings: Settings, args: &RunArgs, cfg: Config) -> Result<Settings> {
        if let Some(s) = args.mac.as_deref() {
            settings.mac = Some(MacAddr::from_str(s).context("Can't parse MAC address!")?);
        } else if let Ok(s) = cfg.get_string("mac")
            && !s.trim().is_empty()
        {
            settings.mac = Some(MacAddr::from_str(&s).context("Can't parse MAC address!")?);
        }

        if let Some(s) = args.ip.as_deref() {
            settings.ip = Some(IpAddr::from_str(s).context("Can't parse IP address!")?);
        } else if let Ok(s) = cfg.get_string("ip")
            && !s.trim().is_empty()
        {
            settings.ip = Some(IpAddr::from_str(&s).context("Can't parse IP address!")?);
        }

        settings.username = args
            .username
            .clone()
            .or_else(|| cfg.get_string("username").ok())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("Username is REQUIRED!"))?;
        settings.password = args
            .password
            .clone()
            .or_else(|| cfg.get_string("password").ok())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("Password is REQUIRED!"))?;

        if let Ok(vs) = cfg.get_array("dns") {
            for value in vs {
                let mut s = value.into_string().context("Invalid DNS server!")?;
                if (s.contains(']') && !s.contains("]:")) || !s.contains(':') {
                    s += ":53";
                }
                let addr = SocketAddr::from_str(&s)
                    .context("Can't parse DNS server to socket address!")?;
                if !settings.dns.contains(&addr) {
                    settings.dns.push(addr);
                }
            }
        }

        if let Some(s) = args.host.clone().or_else(|| cfg.get_string("host").ok())
            && !s.trim().is_empty()
        {
            settings.host = s;
        }

        settings.hostname = args
            .hostname
            .clone()
            .or_else(|| cfg.get_string("hostname").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                hostname::get()
                    .expect("Can't get current computer host name.")
                    .into_string()
                    .expect("Can't parse host name to String.")
            });

        if let Some(s) = args.time.clone().or_else(|| cfg.get_string("time").ok())
            && !s.trim().is_empty()
        {
            settings.time =
                NaiveTime::parse_from_str(&s, "%H:%M").context("Can't parse time String.")?;
        }

        if let Ok(x) = cfg.get_int("reconnect") {
            settings.reconnect = x as u64;
        }

        if let Ok(map) = cfg.get_table("heartbeat") {
            if let Some(x) = get_int_from_map(&map, "eap_timeout") {
                settings.heartbeat.eap_timeout = x as i32;
            }
            if let Some(x) = get_int_from_map(&map, "udp_timeout") {
                settings.heartbeat.udp_timeout = x as i32;
            }
        }

        if let Ok(map) = cfg.get_table("retry") {
            if let Some(x) = get_int_from_map(&map, "count") {
                settings.retry.count = x as i32;
            }
            if let Some(x) = get_int_from_map(&map, "interval") {
                settings.retry.interval = x as i32;
            }
        }

        #[cfg(feature = "log4rs")]
        if settings.log.level_filter != log::LevelFilter::Off
            && let Ok(map) = cfg.get_table("log")
        {
            if let Some(x) = get_bool_from_map(&map, "enable_console") {
                settings.log.enable_console = x;
            }
            if let Some(x) = get_bool_from_map(&map, "enable_file") {
                settings.log.enable_file = x;
            }
            if let Some(x) = get_str_from_map(&map, "file_directory") {
                settings.log.file_directory = x;
            }
            if let Some(Ok(level_filter)) = get_str_from_map(&map, "level")
                .map(|x| log::LevelFilter::from_str(x.to_ascii_uppercase().as_str()))
            {
                settings.log.level_filter = level_filter;
            }
        }

        if let Ok(data) = cfg.get_table("data") {
            if let Some(map) = get_map_from_map(&data, "response_identity")
                && let Some(s) = get_str_from_map(&map, "unknown")
                && let Ok(v) = hex::decode(s)
            {
                settings.data.response_identity.unknown = v;
            }
            if let Some(map) = get_map_from_map(&data, "response_md5_challenge")
                && let Some(s) = get_str_from_map(&map, "unknown")
                && let Ok(v) = hex::decode(s)
            {
                settings.data.response_md5_challenge.unknown = v;
            }
            if let Some(map) = get_map_from_map(&data, "misc_info") {
                if let Some(s) = get_str_from_map(&map, "unknown1")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.unknown1 = v;
                }
                if let Some(s) = get_str_from_map(&map, "cks32_param")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.cks32_param = v;
                }
                if let Some(s) = get_str_from_map(&map, "unknown2")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.unknown2 = v;
                }
                if let Some(s) = get_str_from_map(&map, "os_major")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.os_major = v;
                }
                if let Some(s) = get_str_from_map(&map, "os_minor")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.os_minor = v;
                }
                if let Some(s) = get_str_from_map(&map, "os_build")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.os_build = v;
                }
                if let Some(s) = get_str_from_map(&map, "os_unknown")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.os_unknown = v;
                }
                if let Some(s) = get_str_from_map(&map, "version")
                    && let Ok(v) = hex::decode(s)
                {
                    settings.data.misc_info.version = v;
                }
                if let Some(s) = get_str_from_map(&map, "hash") {
                    settings.data.misc_info.hash = s;
                }
            }
        }

        #[cfg(feature = "log4rs")]
        resolve_log_directory(&mut settings)?;

        Ok(settings)
    }

    fn parse(args: &RunArgs, options: ParseOptions) -> Result<Settings> {
        if matches!(options.create_default_config, false) && !args.config.is_absolute() {
            bail!("Service config path must be absolute.");
        }
        let config_path = absolute_path(&args.config)?;

        let cfg = Settings::read_config(&config_path, options)?;
        let settings = Settings {
            config_path,
            ..Settings::default()
        };
        Settings::resolve(settings, args, cfg)
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("Can't resolve current directory.")?
            .join(path))
    }
}

#[cfg(feature = "log4rs")]
fn resolve_log_directory(settings: &mut Settings) -> Result<()> {
    let path = Path::new(&settings.log.file_directory);
    if path.is_relative() {
        let parent = settings
            .config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        settings.log.file_directory = parent.join(path).to_string_lossy().into_owned();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_service_install_requires_absolute_config() {
        let result = parse_cli_from(["drcom4scut", "service", "install", "--config", "config.yml"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_run_service_requires_absolute_config() {
        let result = parse_cli_from(["drcom4scut", "run-service", "--config", "config.yml"]);
        assert!(result.is_err());
    }
}
