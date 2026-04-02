use anyhow::{Context, Result, anyhow};
use log::{error, info};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::device;
use crate::eap;
use crate::logger;
use crate::settings::{RuntimeMode, Settings};
use crate::socket::{self, Socket};
use crate::udp;
use crate::util::{
    ChannelData, ShutdownSignal, State, sleep_at_with_shutdown, sleep_with_shutdown,
};

pub fn run(
    settings: Settings,
    debug: bool,
    mode: RuntimeMode,
    shutdown: ShutdownSignal,
) -> Result<()> {
    logger::init(&settings, debug, mode);
    log_settings(&settings);

    let settings = Arc::new(settings);
    let device = device::get_device(settings.mac, settings.ip)
        .context("Fail on getting ethernet device!")?;

    info!("Start to run...");
    info!("Ethernet Device: {}", &device.interface.name);
    info!("MAC address: {}", &device.mac);
    info!("IP Address/Prefix: {}", &device.ip_net);
    info!("Username: {}", settings.username);
    info!("Host: {}", settings.host);
    info!("Hostname: {}", settings.hostname);
    info!("Time to wake up: {}", settings.time);
    info!("Reconnect Interval: {}s", settings.reconnect);
    info!(
        "Heartbeat timeout of EAP: {}s",
        settings.heartbeat.eap_timeout
    );
    info!(
        "Heartbeat timeout of UDP: {}s",
        settings.heartbeat.udp_timeout
    );
    info!("Retry Count: {}", settings.retry.count);
    info!("Retry Interval: {}ms", settings.retry.interval);

    let mac = device.mac;
    let ip = device.ip_net.ip();

    let (tx, rx) = crossbeam_channel::unbounded::<ChannelData>();
    let tx1 = tx.clone();
    let eap_settings = settings.clone();
    let eap_shutdown = shutdown.clone();
    let eap_handle = thread::Builder::new()
        .name("EAP-Process-Generator".to_owned())
        .spawn(move || {
            let device = Arc::new(device);
            let mut broke = false;
            loop {
                if eap_shutdown.is_shutdown() {
                    return;
                }
                let mut device = device.clone();
                if broke {
                    info!("Try get the property ethernet device.");
                    loop {
                        if eap_shutdown.is_shutdown() {
                            return;
                        }
                        match device::get_device(Some(mac), Some(ip)) {
                            Ok(d) => {
                                device = Arc::new(d);
                                break;
                            }
                            Err(e) => {
                                error!(
                                    "Can't get ethernet device, try again in {} second(s) : {}",
                                    eap_settings.reconnect, e
                                );
                                if !sleep_with_shutdown(
                                    Duration::from_secs(eap_settings.reconnect),
                                    &eap_shutdown,
                                ) {
                                    return;
                                }
                            }
                        }
                    }
                }
                let tx = tx1.clone();
                let settings = eap_settings.clone();
                let shutdown = eap_shutdown.clone();
                let eap_process = thread::Builder::new()
                    .name("EAP-Process".to_owned())
                    .spawn(move || {
                        info!("Create EAP Process.");
                        let mut eap_process = eap::Process::new(
                            settings.as_ref(),
                            device,
                            tx,
                            shutdown.clone(),
                        );
                        info!("Start EAP Process.");
                        loop {
                            if shutdown.is_shutdown() {
                                break;
                            }
                            match eap_process.start() {
                                State::Sleep => {
                                    error!("Will try reconnect at the next {}.", settings.time);
                                    if sleep_at_with_shutdown(settings.time, &shutdown).is_some() {
                                        continue;
                                    }
                                }
                                State::Quit => {
                                    break;
                                }
                                _ => {
                                    error!(
                                        "Failed at 802.1X Authorization! Will try reconnect in {} second(s).",
                                        settings.reconnect
                                    );
                                }
                            }
                            if !sleep_with_shutdown(
                                Duration::from_secs(settings.reconnect),
                                &shutdown,
                            ) {
                                break;
                            }
                        }
                        info!("Quit EAP Process.");
                    });

                match eap_process {
                    Ok(handle) => {
                        if handle.join().is_err() {
                            error!("Unexpected error at EAP Process thread!");
                        }
                    }
                    Err(e) => {
                        error!("Can't create EAP Process thread: {e}");
                    }
                }

                if eap_shutdown.is_shutdown() {
                    return;
                }

                error!(
                    "Fatal error at EAP Process thread! Will try restart in {} second(s).",
                    eap_settings.reconnect
                );
                if !sleep_with_shutdown(
                    Duration::from_secs(eap_settings.reconnect),
                    &eap_shutdown,
                ) {
                    return;
                }
                broke = true;
            }
        })
        .context("Can't create EAP Process generator thread!")?;

    while !shutdown.is_shutdown() {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(rx_recv) => {
                if matches!(rx_recv.state, State::Success) {
                    tx.send(rx_recv)
                        .context("Can't send initial SUCCESS to UDP process!")?;
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("Unexpected! EAPtoUDP channel is closed."));
            }
        }
    }

    if shutdown.is_shutdown() {
        return Ok(());
    }

    let udp_settings = settings.clone();
    let udp_shutdown = shutdown.clone();
    let udp_handle = thread::Builder::new()
        .name("UDP-Process-Generator".to_owned())
        .spawn(move || {
            loop {
                if udp_shutdown.is_shutdown() {
                    return;
                }
                let rx = rx.clone();
                let settings = udp_settings.clone();
                let shutdown = udp_shutdown.clone();
                let udp_process = thread::Builder::new()
                    .name("UDP-Process".to_owned())
                    .spawn(move || {
                        let (udp_ip, dns) = match socket::resolve_dns(settings.as_ref()) {
                            Some(r) => r,
                            None => {
                                error!("UDP: Can't resolve '{}'.", settings.host);
                                return;
                            }
                        };
                        let socket = Socket::new(match socket::socket_bind(udp_ip) {
                            Some(socket) => socket,
                            None => {
                                error!("UDP: Can't create socket and connect to '{udp_ip}'.");
                                return;
                            }
                        });
                        info!("Create UDP Process.");
                        let mut udp_process = udp::Process::new(
                            settings.as_ref(),
                            Arc::new(socket),
                            rx,
                            mac,
                            ip,
                            dns,
                            shutdown.clone(),
                        );
                        info!("Start UDP Process.");
                        loop {
                            if shutdown.is_shutdown() {
                                break;
                            }
                            match udp_process.start() {
                                State::Sleep => {
                                    error!(
                                        "Will try restart UDP heartbeat at the next {}.",
                                        settings.time
                                    );
                                    if sleep_at_with_shutdown(settings.time, &shutdown).is_some() {
                                        continue;
                                    }
                                }
                                State::Quit => {
                                    break;
                                }
                                _ => {
                                    error!(
                                        "Failed at UDP Process! Will try reconnect in {} second(s).",
                                        settings.reconnect
                                    );
                                }
                            }
                            if !sleep_with_shutdown(
                                Duration::from_secs(settings.reconnect),
                                &shutdown,
                            ) {
                                break;
                            }
                        }
                        info!("Quit UDP Process.");
                    });

                match udp_process {
                    Ok(handle) => {
                        if handle.join().is_err() {
                            error!("Unexpected error at UDP Process thread!");
                        }
                    }
                    Err(e) => {
                        error!("Can't create UDP Process thread: {e}");
                    }
                }

                if udp_shutdown.is_shutdown() {
                    return;
                }

                error!(
                    "Fatal error at UDP Process thread! Will try restart in {} second(s).",
                    udp_settings.reconnect
                );
                if !sleep_with_shutdown(
                    Duration::from_secs(udp_settings.reconnect),
                    &udp_shutdown,
                ) {
                    return;
                }
            }
        })
        .context("Can't create UDP Process generator thread!")?;

    match mode {
        RuntimeMode::Console => {
            udp_handle
                .join()
                .map_err(|_| anyhow!("Fatal error! UDP Process generator thread quit!"))?;
            eap_handle
                .join()
                .map_err(|_| anyhow!("Fatal error! EAP Process generator thread quit!"))?;
        }
        RuntimeMode::Service => {
            while !shutdown.is_shutdown() {
                thread::sleep(Duration::from_millis(200));
            }
        }
    }

    Ok(())
}

fn log_settings(settings: &Settings) {
    for dns in &settings.dns {
        info!("DNS Server: {dns}");
    }

    #[cfg(feature = "log4rs")]
    {
        info!("Log to console: {}", settings.log.enable_console);
        info!("Log to file: {}", settings.log.enable_file);
        info!("Log File Directory: {}", settings.log.file_directory);
        info!("Log Level: {}", settings.log.level_filter);
    }
}
