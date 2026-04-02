use anyhow::Result;

use crate::settings::ServiceCommand;

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use std::ffi::OsString;
    use std::thread;
    use std::time::Duration;
    use windows_service::define_windows_service;
    use windows_service::service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceInfo,
        ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatusHandle,
    };
    use windows_service::service_dispatcher;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    use crate::app;
    use crate::settings::{self, RuntimeMode, ServiceCommand};
    use crate::util::ShutdownSignal;

    const SERVICE_NAME: &str = "drcom4scut";
    const SERVICE_DISPLAY_NAME: &str = "drcom4scut";

    pub fn dispatch() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .context("Failed to start the service dispatcher.")
    }

    pub fn handle(command: ServiceCommand) -> Result<()> {
        match command {
            ServiceCommand::Install { config } => install(config),
            ServiceCommand::Uninstall => uninstall(),
            ServiceCommand::Start => start(),
            ServiceCommand::Stop => stop(),
        }
    }

    fn install(config: std::path::PathBuf) -> Result<()> {
        let manager =
            service_manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
        let service_binary_path =
            std::env::current_exe().context("Can't determine current executable path.")?;
        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: service_binary_path,
            launch_arguments: vec![
                OsString::from("run-service"),
                OsString::from("--config"),
                config.into_os_string(),
            ],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };
        manager
            .create_service(&service_info, ServiceAccess::QUERY_STATUS)
            .context("Failed to create Windows service.")?;
        Ok(())
    }

    fn uninstall() -> Result<()> {
        let manager = service_manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
            )
            .context("Failed to open Windows service.")?;
        let status = service
            .query_status()
            .context("Failed to query service status.")?;
        if status.current_state != ServiceState::Stopped {
            service.stop().context("Failed to stop Windows service.")?;
            wait_for_state(&service, ServiceState::Stopped)?;
        }
        service
            .delete()
            .context("Failed to delete Windows service.")?;
        Ok(())
    }

    fn start() -> Result<()> {
        let manager = service_manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::START,
            )
            .context("Failed to open Windows service.")?;
        service
            .start::<&str>(&[])
            .context("Failed to start Windows service.")?;
        wait_for_state(&service, ServiceState::Running)?;
        Ok(())
    }

    fn stop() -> Result<()> {
        let manager = service_manager(ServiceManagerAccess::CONNECT)?;
        let service = manager
            .open_service(
                SERVICE_NAME,
                ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
            )
            .context("Failed to open Windows service.")?;
        let status = service
            .query_status()
            .context("Failed to query service status.")?;
        if status.current_state != ServiceState::Stopped {
            service.stop().context("Failed to stop Windows service.")?;
            wait_for_state(&service, ServiceState::Stopped)?;
        }
        Ok(())
    }

    fn service_manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
        ServiceManager::local_computer(None::<&str>, access)
            .context("Failed to connect to the Windows service manager.")
    }

    fn wait_for_state(
        service: &windows_service::service::Service,
        expected: ServiceState,
    ) -> Result<()> {
        loop {
            let status = service
                .query_status()
                .context("Failed to query service status.")?;
            if status.current_state == expected {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
    }

    define_windows_service!(ffi_service_main, service_main);

    fn service_main(_arguments: Vec<OsString>) {
        if let Err(error) = run_service() {
            eprintln!("{error:#}");
        }
    }

    fn run_service() -> Result<()> {
        let shutdown = ShutdownSignal::new();
        let status_handle = register_service_handler(shutdown.clone())?;
        set_status(
            &status_handle,
            ServiceState::StartPending,
            ServiceControlAccept::empty(),
        )?;

        let run = settings::parse_run_service()?;

        set_status(
            &status_handle,
            ServiceState::Running,
            ServiceControlAccept::STOP,
        )?;

        let result = app::run(run.settings, run.debug, RuntimeMode::Service, shutdown);
        set_status(
            &status_handle,
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
        )?;
        result
    }

    fn register_service_handler(shutdown: ShutdownSignal) -> Result<ServiceStatusHandle> {
        service_control_handler::register(SERVICE_NAME, move |control_event| match control_event {
            ServiceControl::Stop => {
                shutdown.request_shutdown();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .context("Failed to register service control handler.")
    }

    fn set_status(
        status_handle: &ServiceStatusHandle,
        current_state: ServiceState,
        controls_accepted: ServiceControlAccept,
    ) -> Result<()> {
        status_handle
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state,
                controls_accepted,
                exit_code: windows_service::service::ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::from_secs(5),
                process_id: None,
            })
            .context("Failed to update service status.")
    }
}

#[cfg(not(windows))]
mod imp {
    use anyhow::{Result, bail};

    use crate::settings::ServiceCommand;

    pub fn dispatch() -> Result<()> {
        bail!("Windows services are only supported on Windows.")
    }

    pub fn handle(_command: ServiceCommand) -> Result<()> {
        bail!("Windows services are only supported on Windows.")
    }
}

pub fn dispatch() -> Result<()> {
    imp::dispatch()
}

pub fn handle(command: ServiceCommand) -> Result<()> {
    imp::handle(command)
}
