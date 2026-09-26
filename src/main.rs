use std::collections::HashSet;
use std::io::{self, Write};
use std::time::Duration;

use anyhow::{anyhow, Context as _, Result};
use btleplug::api::{
    Central, CentralEvent, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter,
};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use colored::Colorize;
use futures::stream::StreamExt;
use iqos::{DeviceModel, Iqos, IqosBle};
use serde_json::Value;

mod cli;
mod config;
mod json_output;
mod loader;
mod model_selector;

use cli::{max_attempts, normalize_global_options, scan_timeout, Cli, CliCommand, OneShotCommand, OutputFormat};
use config::{
    normalize_device_label, print_saved_devices, validate_device_label, AppConfig, ConnectedDevice,
};
use loader::parser::{is_invalid_argument_message, CommandError};
use loader::{run_console_with_device, run_registered_command};
use model_selector::parse_device_model;

const EXIT_CONNECTION_FAILED: i32 = 1;
const EXIT_INVALID_ARGUMENTS: i32 = 2;
const EXIT_DEVICE_COMMAND_FAILED: i32 = 3;
const EXIT_LABEL_NOT_FOUND: i32 = 4;

/// Delay between reconnect attempts when a connection or command fails with
/// a retriable (transport/connection) error.
const RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Default polling interval for `battery --watch` when `--interval` is omitted.
const DEFAULT_WATCH_INTERVAL_SECS: u64 = 2;

#[derive(Debug)]
struct ExitError {
    code: i32,
    error: anyhow::Error,
}

impl ExitError {
    fn new(code: i32, error: impl Into<anyhow::Error>) -> Self {
        Self {
            code,
            error: error.into(),
        }
    }
}

#[derive(Debug, Clone)]
enum ScanTarget {
    Model(DeviceModel),
    Address {
        label: Option<String>,
        address: String,
        cached_serial: Option<String>,
    },
}

#[derive(Debug)]
struct DiscoveredDevice {
    address: String,
    local_name: Option<String>,
}

#[derive(Debug)]
struct ResolvedTarget {
    config: AppConfig,
    target: ScanTarget,
    should_save_memory: bool,
}

async fn get_central(manager: &Manager) -> Result<Adapter> {
    manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .context("No Bluetooth adapters found")
}

async fn prompt_for_connection(name: &str, addr: &str) -> Result<bool> {
    let prompt = format!("Connect to {name} ({addr})? [y/n]: ");

    tokio::task::spawn_blocking(move || loop {
        print!("{prompt}");
        io::stdout().flush()?;

        let mut input = String::new();
        let n = io::stdin().read_line(&mut input)?;
        if n == 0 {
            return Ok(false);
        }

        match input.trim() {
            answer if answer.eq_ignore_ascii_case("y") => return Ok(true),
            answer if answer.eq_ignore_ascii_case("n") => return Ok(false),
            _ => {}
        }
    })
    .await?
}

const IQOS_CLI_ASCII_ART: &str = r"

 ██╗  ██████╗   ██████╗  ███████╗      ██████╗ ██╗      ██╗
 ██║ ██╔═══██╗ ██╔═══██╗ ██╔════╝     ██╔════╝ ██║      ██║
 ██║ ██║   ██║ ██║   ██║ ███████╗     ██║      ██║      ██║
 ██║ ██║▄▄ ██║ ██║   ██║ ╚════██║     ██║      ██║      ██║
 ██║ ╚██████╔╝ ╚██████╔╝ ███████║     ╚██████╗ ███████╗ ██║
 ╚═╝  ╚══▀▀═╝   ╚═════╝  ╚══════╝      ╚═════╝ ╚══════╝ ╚═╝

";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let exit_code = if cli::should_use_cli(&args) {
        run_cli(args).await
    } else {
        match run_interactive().await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Error: {error:#}");
                EXIT_CONNECTION_FAILED
            }
        }
    };

    std::process::exit(exit_code);
}

async fn run_cli(mut args: Vec<String>) -> i32 {
    if cli::has_version_flag(&args) {
        cli::print_version();
        return 0;
    }

    if let Some(program) = args.first_mut() {
        *program = "iqos".to_string();
    }
    let args = normalize_global_options(args);

    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return code;
        }
    };

    if cli.version {
        cli::print_version();
        return 0;
    }

    if let Some(CliCommand::Completions { shell }) = &cli.command {
        print_completions(*shell);
        return 0;
    }

    let attempts = max_attempts(cli.retries);

    let watch_interval = if let Some(CliCommand::Battery {
        watch: true,
        interval,
    }) = &cli.command
    {
        Some(Duration::from_secs(
            interval.unwrap_or(DEFAULT_WATCH_INTERVAL_SECS).max(1),
        ))
    } else {
        None
    };

    if let Some(interval) = watch_interval {
        return match run_battery_watch(cli.model, scan_timeout(cli.timeout), interval, attempts)
            .await
        {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Error: {:#}", error.error);
                error.code
            }
        };
    }

    let Some(command) = cli.command else {
        return match run_auto_connected_console(cli.model, scan_timeout(cli.timeout), attempts)
            .await
        {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Error: {:#}", error.error);
                error.code
            }
        };
    };

    match run_one_shot(
        cli.model,
        scan_timeout(cli.timeout),
        command.into_one_shot(),
        cli.format,
        attempts,
    )
    .await
    {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Error: {:#}", error.error);
            error.code
        }
    }
}

fn print_completions(shell: Shell) {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    generate(shell, &mut command, name, &mut io::stdout());
}

async fn run_auto_connected_console(
    model_arg: Option<String>,
    timeout: Duration,
    attempts: u32,
) -> std::result::Result<(), ExitError> {
    print_ascii_art();

    let ResolvedTarget {
        mut config,
        target,
        should_save_memory,
    } = load_config_and_resolve_target(model_arg.as_deref(), true)?;
    let (iqos, device) = connect_target_with_retry(&target, timeout, attempts).await?;

    apply_connection_memory(&mut config, &target, &device);
    save_connection_memory(&config, &target, should_save_memory, true)?;

    run_console_with_device(Iqos::new(iqos), device)
        .await
        .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))
}

/// Connect to `target`, retrying on connection/transport failures up to
/// `attempts` times with a fixed backoff between tries. Non-retriable
/// failures (e.g. invalid arguments) are returned immediately.
async fn connect_target_with_retry(
    target: &ScanTarget,
    timeout: Duration,
    attempts: u32,
) -> std::result::Result<(IqosBle, ConnectedDevice), ExitError> {
    let mut last_error: Option<ExitError> = None;
    for attempt in 1..=attempts {
        match connect_target(target, timeout).await {
            Ok(result) => return Ok(result),
            Err(error) if attempt < attempts && is_retriable(error.code) => {
                eprintln!(
                    "Warning: connection attempt {attempt}/{attempts} failed: {:#} (retrying in {}s)",
                    error.error,
                    RETRY_BACKOFF.as_secs()
                );
                last_error = Some(error);
                tokio::time::sleep(RETRY_BACKOFF).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("loop returns before exhausting attempts unless an error was recorded"))
}

fn is_retriable(code: i32) -> bool {
    code == EXIT_CONNECTION_FAILED || code == EXIT_DEVICE_COMMAND_FAILED
}

/// Connect once, then poll the battery level on an interval until the
/// process is interrupted (Ctrl+C).
async fn run_battery_watch(
    model_arg: Option<String>,
    timeout: Duration,
    interval: Duration,
    attempts: u32,
) -> std::result::Result<(), ExitError> {
    let ResolvedTarget {
        mut config,
        target,
        should_save_memory,
    } = load_config_and_resolve_target(model_arg.as_deref(), true)?;
    let (ble, device) = connect_target_with_retry(&target, timeout, attempts).await?;
    apply_connection_memory(&mut config, &target, &device);
    save_connection_memory(&config, &target, should_save_memory, true)?;

    println!(
        "Watching battery level every {}s. Press Ctrl+C to stop.",
        interval.as_secs()
    );
    let start = std::time::Instant::now();
    loop {
        match ble.read_battery_level().await {
            Ok(level) => println!("[+{:>5}s] Battery: {level}%", start.elapsed().as_secs()),
            Err(error) => eprintln!("Warning: battery read failed: {error:#}"),
        }
        tokio::time::sleep(interval).await;
    }
}

async fn run_one_shot(
    model_arg: Option<String>,
    timeout: Duration,
    command: OneShotCommand,
    format: OutputFormat,
    attempts: u32,
) -> std::result::Result<(), ExitError> {
    match command {
        OneShotCommand::DeviceList => {
            let config = AppConfig::load()
                .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
            if format == OutputFormat::Json {
                print_json(
                    serde_json::to_value(&config)
                        .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?,
                )?;
            } else {
                print_saved_devices(&config);
            }
            Ok(())
        }
        OneShotCommand::DeviceRemove { label } => {
            let label = normalize_device_label(&label)
                .map_err(|error| ExitError::new(EXIT_INVALID_ARGUMENTS, error))?;
            let mut config = AppConfig::load()
                .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
            if config
                .remove_device(&label)
                .map_err(|error| ExitError::new(EXIT_INVALID_ARGUMENTS, error))?
            {
                config
                    .save()
                    .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
                println!("Removed device label: {label}");
                Ok(())
            } else {
                Err(ExitError::new(
                    EXIT_LABEL_NOT_FOUND,
                    anyhow!("Device label not found: {label}"),
                ))
            }
        }
        OneShotCommand::DeviceSave { label } => {
            let label = validate_device_label(&label)
                .map_err(|error| ExitError::new(EXIT_INVALID_ARGUMENTS, error))?;
            let ResolvedTarget {
                mut config, target, ..
            } = load_config_and_resolve_target(model_arg.as_deref(), false)?;
            let (iqos, device) = connect_target_with_retry(&target, timeout, attempts).await?;
            apply_connection_memory(&mut config, &target, &device);
            config
                .save_device(label.clone(), &device)
                .map_err(|error| ExitError::new(EXIT_INVALID_ARGUMENTS, error))?;
            config
                .save()
                .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
            drop(iqos);
            println!("Saved device label: {label}");
            Ok(())
        }
        OneShotCommand::Registered { name, args } => {
            let ResolvedTarget {
                config: mut command_config,
                target,
                should_save_memory,
            } = load_config_and_resolve_target(model_arg.as_deref(), true)?;

            let mut last_error: Option<ExitError> = None;
            for attempt in 1..=attempts {
                let outcome = run_registered_once(
                    &target,
                    timeout,
                    name,
                    &args,
                    format,
                    &mut command_config,
                    should_save_memory,
                )
                .await;

                match outcome {
                    Ok(()) => return Ok(()),
                    Err(error) if attempt < attempts && is_retriable(error.code) => {
                        eprintln!(
                            "Warning: {:#} (attempt {attempt}/{attempts}, retrying in {}s)",
                            error.error,
                            RETRY_BACKOFF.as_secs()
                        );
                        last_error = Some(error);
                        tokio::time::sleep(RETRY_BACKOFF).await;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(last_error
                .expect("loop returns before exhausting attempts unless an error was recorded"))
        }
    }
}

/// Connect once and run `name` with `args`, or — when `format` is
/// [`OutputFormat::Json`] and `name` is a supported data command — fetch the
/// same data directly and print it as JSON instead of routing through the
/// text-oriented command registry.
async fn run_registered_once(
    target: &ScanTarget,
    timeout: Duration,
    name: &'static str,
    args: &[String],
    format: OutputFormat,
    command_config: &mut AppConfig,
    should_save_memory: bool,
) -> std::result::Result<(), ExitError> {
    let (ble, device) = connect_target(target, timeout).await?;
    apply_connection_memory(command_config, target, &device);
    save_connection_memory(command_config, target, should_save_memory, true)?;

    if format == OutputFormat::Json {
        match name {
            "battery" => {
                let level = ble
                    .read_battery_level()
                    .await
                    .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
                return print_json(json_output::battery(level));
            }
            "info" => {
                let model = ble.model();
                let device_info = ble.device_info().clone();
                let iqos = Iqos::new(ble);
                let status = iqos
                    .read_device_status(model, device_info)
                    .await
                    .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
                return print_json(json_output::device_status(&status));
            }
            "diagnosis" => {
                let iqos = Iqos::new(ble);
                let data = iqos
                    .read_diagnosis()
                    .await
                    .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
                return print_json(json_output::diagnosis(&data));
            }
            _ => {}
        }
    }

    run_registered_command(Iqos::new(ble), name, args.to_vec())
        .await
        .map_err(|error| ExitError::new(classify_command_error(&error), error))
}

fn print_json(value: Value) -> std::result::Result<(), ExitError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| ExitError::new(EXIT_DEVICE_COMMAND_FAILED, error))?;
    println!("{text}");
    Ok(())
}

fn load_config_and_resolve_target(
    model_arg: Option<&str>,
    allow_model_config_failure: bool,
) -> std::result::Result<ResolvedTarget, ExitError> {
    if let Some(value) = model_arg {
        if let Some(model) = parse_device_model(value) {
            let (config, should_save_memory) = load_memory_config(allow_model_config_failure)?;
            return Ok(ResolvedTarget {
                config,
                target: ScanTarget::Model(model),
                should_save_memory,
            });
        }
    }

    let config =
        AppConfig::load().map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;
    let target = resolve_target(model_arg, &config)?;

    Ok(ResolvedTarget {
        config,
        target,
        should_save_memory: true,
    })
}

fn load_memory_config(allow_failure: bool) -> std::result::Result<(AppConfig, bool), ExitError> {
    match AppConfig::load() {
        Ok(config) => Ok((config, true)),
        Err(error) if allow_failure => {
            eprintln!("Warning: could not load device config: {error:#}");
            Ok((AppConfig::default(), false))
        }
        Err(error) => Err(ExitError::new(EXIT_CONNECTION_FAILED, error)),
    }
}

fn resolve_target(
    model_arg: Option<&str>,
    config: &AppConfig,
) -> std::result::Result<ScanTarget, ExitError> {
    match model_arg {
        Some(value) => {
            if let Some(model) = parse_device_model(value) {
                return Ok(ScanTarget::Model(model));
            }
            let label = value.trim();

            let saved = config.devices.get(label).ok_or_else(|| {
                ExitError::new(
                    EXIT_LABEL_NOT_FOUND,
                    anyhow!("Device label not found: {label}"),
                )
            })?;

            Ok(ScanTarget::Address {
                label: Some(label.to_string()),
                address: saved.address.clone(),
                cached_serial: saved.serial_number.clone(),
            })
        }
        None => {
            let default = config.default.as_ref().ok_or_else(|| {
                ExitError::new(
                    EXIT_LABEL_NOT_FOUND,
                    anyhow!("No default device stored. Connect interactively or use --model <DeviceModel>."),
                )
            })?;

            Ok(ScanTarget::Address {
                label: None,
                address: default.address.clone(),
                cached_serial: None,
            })
        }
    }
}

fn save_connection_memory(
    config: &AppConfig,
    target: &ScanTarget,
    should_save_memory: bool,
    allow_model_save_failure: bool,
) -> std::result::Result<(), ExitError> {
    if !should_save_memory {
        return Ok(());
    }

    match config.save() {
        Ok(()) => Ok(()),
        Err(error) if allow_model_save_failure && matches!(target, ScanTarget::Model(_)) => {
            eprintln!("Warning: could not save device config: {error:#}");
            Ok(())
        }
        Err(error) => Err(ExitError::new(EXIT_CONNECTION_FAILED, error)),
    }
}

async fn connect_target(
    target: &ScanTarget,
    timeout: Duration,
) -> std::result::Result<(IqosBle, ConnectedDevice), ExitError> {
    let manager = Manager::new()
        .await
        .map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;
    let central = get_central(&manager)
        .await
        .map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;

    let (peripheral, discovered) = find_matching_peripheral(&central, target, timeout).await?;

    let ble = IqosBle::connect_and_discover(peripheral)
        .await
        .map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;
    let device = connected_device(&ble, discovered);
    warn_serial_mismatch(target, &device);

    Ok((ble, device))
}

async fn find_matching_peripheral(
    central: &Adapter,
    target: &ScanTarget,
    timeout: Duration,
) -> std::result::Result<(Peripheral, DiscoveredDevice), ExitError> {
    let mut events = central
        .events()
        .await
        .map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;

    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|error| ExitError::new(EXIT_CONNECTION_FAILED, error))?;

    let result = tokio::time::timeout(timeout, async {
        while let Some(event) = events.next().await {
            let addr = match event {
                CentralEvent::DeviceDiscovered(addr) | CentralEvent::DeviceUpdated(addr) => addr,
                _ => continue,
            };

            let peripheral = match central.peripheral(&addr).await {
                Ok(peripheral) => peripheral,
                Err(error) => {
                    eprintln!("Warning: could not query peripheral {addr}: {error}");
                    continue;
                }
            };
            let properties = match peripheral.properties().await {
                Ok(properties) => properties,
                Err(error) => {
                    eprintln!("Warning: could not read properties for {addr}: {error}");
                    continue;
                }
            };
            let discovered = discovered_device(&addr, properties.as_ref());

            if target_matches(target, &discovered) {
                return Ok::<_, anyhow::Error>(Some((peripheral, discovered)));
            }
        }

        Ok::<_, anyhow::Error>(None)
    })
    .await;

    if let Err(error) = central.stop_scan().await {
        eprintln!("Warning: could not stop BLE scan: {error}");
    }

    match result {
        Ok(Ok(Some(found))) => Ok(found),
        Ok(Ok(None)) => Err(ExitError::new(
            EXIT_CONNECTION_FAILED,
            anyhow!("BLE event stream ended before a matching IQOS device was found"),
        )),
        Ok(Err(error)) => Err(ExitError::new(EXIT_CONNECTION_FAILED, error)),
        Err(_) => Err(ExitError::new(
            EXIT_CONNECTION_FAILED,
            anyhow!(
                "Device not found before scan timeout: {}",
                describe_target(target)
            ),
        )),
    }
}

async fn run_interactive() -> Result<()> {
    let manager = Manager::new().await?;
    print_ascii_art();

    let central = get_central(&manager).await?;
    let central_state = central.adapter_state().await?;
    println!("CentralState: {:?}", central_state);

    let mut events = central.events().await?;
    let mut ignore_devices = HashSet::<String>::new();

    println!("Scanning for IQOS devices...");
    central.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(addr) => {
                let peripheral = central.peripheral(&addr).await?;
                let properties = peripheral.properties().await?;
                let discovered = discovered_device(&addr, properties.as_ref());
                let name = discovered.local_name.clone().unwrap_or_default();

                if name.contains("IQOS") && !ignore_devices.contains(&discovered.address) {
                    println!("Found IQOS: {name} ({})", discovered.address);

                    if prompt_for_connection(&name, &discovered.address).await? {
                        println!("Connecting...");
                        let ble = IqosBle::connect_and_discover(peripheral).await?;
                        let device = connected_device(&ble, discovered);
                        remember_connected_device(&device);
                        let iqos = Iqos::new(ble);
                        central.stop_scan().await?;
                        run_console_with_device(iqos, device).await?;
                        return Ok(());
                    }

                    ignore_devices.insert(discovered.address);
                    println!("Scanning for other devices...");
                }
            }
            CentralEvent::StateUpdate(state) => println!("State Update: {:?}", state),
            CentralEvent::DeviceConnected(id) => println!("Device Connected: {id}"),
            CentralEvent::DeviceDisconnected(id) => println!("Device Disconnected: {id}"),
            _ => {}
        }
    }

    Ok(())
}

fn print_ascii_art() {
    println!("{}", IQOS_CLI_ASCII_ART.blue());
}

fn remember_connected_device(device: &ConnectedDevice) {
    match AppConfig::load() {
        Ok(mut config) => {
            config.update_default(device);
            if let Err(error) = config.save() {
                eprintln!("Warning: could not save device config: {error:#}");
            }
        }
        Err(error) => eprintln!("Warning: could not load device config: {error:#}"),
    }
}

fn discovered_device(
    addr: &PeripheralId,
    properties: Option<&PeripheralProperties>,
) -> DiscoveredDevice {
    let local_name = properties.and_then(|properties| properties.local_name.clone());
    let address = properties
        .map(|properties| properties.address.to_string())
        .filter(|address| address != "00:00:00:00:00:00")
        .unwrap_or_else(|| addr.to_string());

    DiscoveredDevice {
        address,
        local_name,
    }
}

fn connected_device(ble: &IqosBle, discovered: DiscoveredDevice) -> ConnectedDevice {
    ConnectedDevice {
        address: discovered.address,
        local_name: discovered.local_name,
        model: ble.model(),
        serial_number: ble.device_info().serial_number.clone(),
    }
}

fn target_matches(target: &ScanTarget, discovered: &DiscoveredDevice) -> bool {
    match target {
        ScanTarget::Model(model) => {
            discovered
                .local_name
                .as_deref()
                .map(DeviceModel::from_local_name)
                == Some(*model)
        }
        ScanTarget::Address { address, .. } => discovered.address.eq_ignore_ascii_case(address),
    }
}

fn apply_connection_memory(config: &mut AppConfig, target: &ScanTarget, device: &ConnectedDevice) {
    config.update_default(device);

    if let ScanTarget::Address {
        label: Some(label), ..
    } = target
    {
        config.update_saved_device_metadata(label, device);
    }
}

fn warn_serial_mismatch(target: &ScanTarget, device: &ConnectedDevice) {
    let ScanTarget::Address {
        label,
        cached_serial: Some(cached_serial),
        ..
    } = target
    else {
        return;
    };

    let Some(actual_serial) = &device.serial_number else {
        return;
    };

    if cached_serial != actual_serial {
        let target_name = label.as_deref().unwrap_or("default");
        eprintln!(
            "Warning: serial number mismatch for {target_name}: cached {cached_serial}, connected {actual_serial}"
        );
    }
}

fn classify_command_error(error: &anyhow::Error) -> i32 {
    if let Some(command_error) = error.downcast_ref::<CommandError>() {
        return match command_error {
            CommandError::InvalidArguments(_) => EXIT_INVALID_ARGUMENTS,
            CommandError::DeviceFailure(_) => EXIT_DEVICE_COMMAND_FAILED,
        };
    }

    if is_invalid_argument_message(&error.to_string()) {
        EXIT_INVALID_ARGUMENTS
    } else {
        EXIT_DEVICE_COMMAND_FAILED
    }
}

fn describe_target(target: &ScanTarget) -> String {
    match target {
        ScanTarget::Model(model) => format!("{model:?}"),
        ScanTarget::Address {
            label: Some(label),
            address,
            ..
        } => format!("{label} ({address})"),
        ScanTarget::Address { address, .. } => address.clone(),
    }
}
