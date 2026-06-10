use crate::ui_cm_interface::{start_ipc, ConnectionManager, InvokeUiCM};
use hbb_common::allow_err;
use sciter::{make_args, Element, Value, HELEMENT};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use clipboard::ContextSend;
use hbb_common::log;
#[cfg(target_os = "linux")]
use crate::ipc::start_pa;

#[derive(Clone, Default)]
pub struct SciterHandler {
    pub element: Arc<Mutex<Option<Element>>>,
}

impl InvokeUiCM for SciterHandler {
    fn add_connection(&self, client: &crate::ui_cm_interface::Client, security_numbers: String, avatar_image: String) {
        self.call(
            "addConnection",
            &make_args!(
                client.id,
                client.is_file_transfer,
                client.is_view_camera,
                client.port_forward.clone(),
                client.peer_id.clone(),
                client.name.clone(),
                client.authorized,
                client.keyboard,
                client.clipboard,
                client.audio,
                client.file,
                client.restart,
                client.recording,
                client.block_input,
                client.camera,
                security_numbers,
                avatar_image,
                client.from_switch,
                client.is_invite
            ),
        );
    }

    fn remove_connection(&self, id: i32, close: bool) {
        self.call("removeConnection", &make_args!(id, close));
        if crate::ui_cm_interface::get_clients_length().eq(&0) {
            crate::platform::quit_gui();
        }
    }

    fn new_message(&self, id: i32, text: String) {
        self.call("newMessage", &make_args!(id, text));
    }

    fn change_theme(&self, _dark: String) {
        // TODO
    }

    fn change_language(&self) {
        // TODO
    }

    fn show_elevation(&self, show: bool) {
        self.call("showElevation", &make_args!(show));
    }

    fn update_voice_call_state(&self, client: &crate::ui_cm_interface::Client) {
        self.call(
            "updateVoiceCallState",
            &make_args!(client.id, client.in_voice_call, client.incoming_voice_call),
        );
    }

    fn update_link_dashboard_state(&self, client: &crate::ui_cm_interface::Client) {
        self.call(
            "updateLinkDashboardState",
            &make_args!(
                client.id,
                client.incoming_link_dashboard,
                client.link_dashboard_account_name.clone(),
                client.link_dashboard_existing_account_name.clone()
            ),
        );
    }

    fn file_transfer_log(&self, action: &str, log: &str) {
        self.call("file_transfer_log", &make_args!(action, log));
    }

    fn accept_invite(&self, id: i32) {
        self.call("acceptInvite", &make_args!(id));
    }

    fn decline_invite(&self, id: i32) {
        self.call("declineInvite", &make_args!(id));
    }
}

impl SciterHandler {
    #[inline]
    fn call(&self, func: &str, args: &[Value]) {
        if let Some(e) = self.element.lock().unwrap().as_ref() {
            allow_err!(e.call_method(func, &super::value_crash_workaround(args)[..]));
        }
    }
}

pub struct SciterConnectionManager(ConnectionManager<SciterHandler>);

impl Deref for SciterConnectionManager {
    type Target = ConnectionManager<SciterHandler>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SciterConnectionManager {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            // Ensure clipboard context is reset at the very beginning of CM creation.
            // This prevents a race condition where a lingering active context from a 
            // previous session causes the new CM to immediately send a clipboard-related
            // message, which breaks the invite flow.
            log::info!("[CM] Resetting clipboard context state at initialization.");
            ContextSend::set_is_stopped();
        }

        #[cfg(target_os = "linux")]
        std::thread::spawn(start_pa);
        let cm = ConnectionManager {
            ui_handler: SciterHandler::default(),
        };
        let cloned = cm.clone();
        std::thread::spawn(move || start_ipc(cloned));
        SciterConnectionManager(cm)
    }

    fn get_icon(&mut self) -> String {
        super::get_icon()
    }

    fn check_click_time(&mut self, id: i32) {
        crate::ui_cm_interface::check_click_time(id);
    }

    fn get_click_time(&self) -> f64 {
        crate::ui_cm_interface::get_click_time() as _
    }

    fn switch_permission(&self, id: i32, name: String, enabled: bool) {
        crate::ui_cm_interface::switch_permission(id, name, enabled);
    }

    fn close(&self, id: i32) {
		crate::ui_cm_interface::close(id);
    }

    fn remove_disconnected_connection(&self, id: i32) {
        crate::ui_cm_interface::remove(id);
    }

    fn quit(&self) {
        log::info!("[CM quit] Closing all client connections before quit");
        crate::ui_cm_interface::close_all_clients();
        // Allow time for Data::Close to propagate through IPC to server connections
        log::info!("[CM quit] Sleeping 150ms to let Data::Close propagate");
        std::thread::sleep(std::time::Duration::from_millis(150));
        log::info!("[CM quit] Calling quit_gui()");
        crate::platform::quit_gui();
    }

    fn authorize(&self, id: i32) {
        crate::ui_cm_interface::authorize(id);
    }

    fn send_msg(&self, id: i32, text: String) {
        crate::ui_cm_interface::send_chat(id, text);
    }

    fn t(&self, name: String) -> String {
        crate::client::translate(name)
    }

    fn can_elevate(&self) -> bool {
        crate::ui_cm_interface::can_elevate()
    }
    
    fn elevate_portable(&self, id: i32) {
        crate::ui_cm_interface::elevate_portable(id);
    }

    fn get_option(&self, key: String) -> String {
        crate::ui_interface::get_option(key)
    }    

    fn accept_invite(&mut self, id: i32) {
        log::info!("Invite accepted for connection id {}", id);
        if let Some(client) = crate::ui_cm_interface::get_clients_lock().ok().and_then(|clients| clients.get(&id).cloned()) {
            // Tell the server process to send an InviteResponse(accepted=true)
            if let Err(e) = client.get_tx().send(crate::ipc::Data::InviteResponse { id, accepted: true }) {
                log::error!("Failed to send accept response for invite: {}", e);
            }

            // Spawn new process to connect back
            log::info!("Spawning new process to connect to inviter {} with stored password.", client.peer_id);
            if let Ok(exe) = std::env::current_exe() {
                let mut cmd = std::process::Command::new(exe);
                cmd.arg("--connect");
                cmd.arg(client.peer_id);
                cmd.arg("--password");
                cmd.arg(client.password_to_connect_to_inviter);
                if let Err(e) = cmd.spawn() {
                    log::error!("Failed to spawn new process for invited connection: {}", e);
                }
            }
        } else {
            log::error!("Could not find client details for accepted invite id {}", id);
        }
        // Close the current invite connection window
        crate::ui_cm_interface::close(id);
    }

    fn decline_invite(&mut self, id: i32) {
        log::info!("Invite declined for connection id {}", id);
        if let Some(client) = crate::ui_cm_interface::get_clients_lock().ok().and_then(|clients| clients.get(&id).cloned()) {
            // Tell the server process to send an InviteResponse(accepted=false)
            if let Err(e) = client.get_tx().send(crate::ipc::Data::InviteResponse { id, accepted: false }) {
                log::error!("Failed to send decline response for invite: {}", e);
            }
        } else {
            log::error!("Could not find client details for declined invite id {}", id);
        }
        // Close the current invite connection window
        crate::ui_cm_interface::close(id);
    }

    fn answer_link_dashboard(&self, id: i32, accept: bool) {
        crate::ui_cm_interface::handle_link_dashboard_response(id, accept);
    }
}

impl sciter::EventHandler for SciterConnectionManager {
    fn attached(&mut self, root: HELEMENT) {
        *self.ui_handler.element.lock().unwrap() = Some(Element::from(root));
    }

    sciter::dispatch_script_call! {
        fn t(String);
        fn check_click_time(i32);
        fn get_click_time();
        fn get_icon();
        fn close(i32);
        fn remove_disconnected_connection(i32);
        fn quit();
        fn authorize(i32);
        fn switch_permission(i32, String, bool);
        fn send_msg(i32, String);
        fn can_elevate();
        fn elevate_portable(i32);
        fn get_option(String);
        fn accept_invite(i32);
        fn decline_invite(i32);
        fn answer_link_dashboard(i32, bool);
    }

    fn on_script_call(&mut self, root: HELEMENT, name: &str, args: &[Value]) -> Option<Value> {
        self.dispatch_script_call(root, name, args)
    }
}
