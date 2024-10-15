// Copyright Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![deny(warnings, clippy::all)]

use adw::prelude::*;
use gtk::gio;
use gtk::gio::SimpleAction;
use gtk::glib;
use gtk::glib::Variant;
use model::{Device, Devices};
use services::{StorageService, StorageServiceClient};
use widgets::TurnOnApplicationWindow;

mod i18n;
mod model;
mod net;
mod services;
mod widgets;

static APP_ID: &str = "de.swsnr.turnon";

fn activate_about_action(app: &adw::Application, _action: &SimpleAction, _param: Option<&Variant>) {
    adw::AboutDialog::from_appdata(
        "/de/swsnr/turnon/de.swsnr.turnon.metainfo.xml",
        Some(env!("CARGO_PKG_VERSION")),
    )
    .present(app.active_window().as_ref());
}

fn save_automatically(model: &Devices, storage: StorageServiceClient) {
    model.connect_items_changed(move |model, pos, n_added, _| {
        log::debug!("Device list changed, saving devices");
        storage.request_save_devices(model.into());
        // Persist devices whenever one device changes
        for n in pos..n_added {
            model.item(n).unwrap().connect_notify_local(
                None,
                glib::clone!(
                    #[strong]
                    storage,
                    #[weak]
                    model,
                    move |_, _| {
                        log::debug!("One device was changed, saving devices");
                        storage.request_save_devices((&model).into());
                    }
                ),
            );
        }
    });
}

/// Handle application startup.
///
/// Create application actions.
fn startup_application(app: &adw::Application, model: &Devices) {
    log::debug!("Application starting");
    gtk::Window::set_default_icon_name(APP_ID);

    let actions = [
        gio::ActionEntryBuilder::new("quit")
            .activate(|a: &adw::Application, _, _| a.quit())
            .build(),
        gio::ActionEntryBuilder::new("about")
            .activate(activate_about_action)
            .build(),
    ];
    app.add_action_entries(actions);

    app.set_accels_for_action("win.add_device", &["<Control>n"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);

    log::debug!("Initializing storage");
    let data_dir = glib::user_data_dir().join(APP_ID);
    let storage = StorageService::new(data_dir.join("devices.json"));

    log::info!("Loading devices synchronously");
    let devices = match storage.load_sync() {
        Err(error) => {
            log::error!(
                "Failed to load devices from {}: {}",
                storage.target().display(),
                error
            );
            Vec::new()
        }
        Ok(devices) => devices.into_iter().map(Device::from).collect(),
    };
    model.reset_devices(devices);
    save_automatically(model, storage.client());
    glib::spawn_future_local(storage.spawn());
}

fn activate_application(app: &adw::Application, model: &Devices) {
    match app.active_window() {
        Some(window) => {
            log::debug!("Representing existing application window");
            window.present()
        }
        None => {
            log::debug!("Creating new application window");
            TurnOnApplicationWindow::new(app, model).present();
        }
    }
}

/// Set up logging.
///
/// If the process is connected to journald log structured events directly to journald.
///
/// Otherwise log to console.
///
/// `$TURNON_LOG` and `$TURNON_LOG_STYLE` configure log level and log style (for console logging)
fn setup_logging() {
    let env_var = "TURNON_LOG";
    if systemd_journal_logger::connected_to_journal() {
        let logger = systemd_journal_logger::JournalLog::new()
            .unwrap()
            .with_extra_fields([("VERSION", env!("CARGO_PKG_VERSION"))]);
        let filter = env_filter::Builder::from_env(env_var).build();
        let max_level = filter.filter();
        log::set_boxed_logger(Box::new(env_filter::FilteredLog::new(logger, filter))).unwrap();
        log::set_max_level(max_level);
    } else {
        let env = env_logger::Env::new()
            .filter(env_var)
            .write_style("TURNON_LOG_STYLE");
        env_logger::init_from_env(env);
    }
    glib::log_set_default_handler(glib::rust_log_handler);
}

fn main() -> glib::ExitCode {
    setup_logging();

    gio::resources_register_include!("turnon.gresource").unwrap();
    glib::set_application_name("TurnOn");

    let app = adw::Application::builder().application_id(APP_ID).build();

    let model = Devices::default();

    app.connect_activate(glib::clone!(
        #[strong]
        model,
        move |app| activate_application(app, &model)
    ));
    app.connect_startup(glib::clone!(
        #[strong]
        model,
        move |app| startup_application(app, &model)
    ));

    app.run()
}
