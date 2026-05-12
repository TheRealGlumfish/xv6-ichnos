mod ui;
mod xv6;

use xv6::memory::MemoryInfo;
use xv6::process::Process;
use xv6::rpc::{MAGIC, RpcHandler, RpcReq, RpcResp};
use xv6::syscall::Syscall;

use iced::futures::{SinkExt, Stream, StreamExt, channel::mpsc};
use iced::stream;
use iced::task::Task;
use iced::time;
use iced::widget::{center, column, container, row, text};
use iced::{self, Alignment, Element, Length, Subscription};
use iced_aw::{ICED_AW_FONT_BYTES, TabLabel, Tabs};
use iced_fonts::CODICON_FONT_BYTES;
use tokio::net::UnixStream;
use tokio::time::sleep;

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::time::Duration;

// TODO: Add a magic/heartbeat mechanism to detect if the xv6 connection is alive
// If broken prompt the user to restart, flush the request queue and reconnect.
// TODO: Add proper shutdown behavior, we want the app to exit gracefully if the user exits it (or due to some error),
// specifically flush RPC responses so we don't leave the xv6 side hanging.
// TODO: Remove the various unwraps and funnel errors through <dyn Error>
// TODO: Disable all buttons in the "Disconnected" state

const RPC_CONNECT_RETRY_DELAY: Duration = Duration::from_secs(5);
const RPC_CONNECT_MAX_ATTEMPTS: usize = 10;

struct TracedProcess {
    process: Process,
    trace_mask: u32,
    trace_events: Vec<Syscall>,
    is_alive: bool,
    memory: Option<MemoryInfo>,
}

impl TracedProcess {
    fn new(process: Process) -> Self {
        Self {
            process,
            trace_mask: 0,
            trace_events: Vec::new(),
            is_alive: true,
            memory: None,
        }
    }
}

impl TracedProcess {
    fn is_traced(&self) -> bool {
        self.trace_mask != 0
    }
}

#[derive(Clone, PartialEq, Eq)]
enum TabId {
    Syscalls,
    Memory,
}

#[derive(Clone)]
enum Message {
    Exit,
    Connected(mpsc::Sender<RpcReq>),
    RequestHeartbeat,
    RequestMagic,
    // Errors (from UI or RPC worker)
    ExitError(String),
    RpcError(String, bool),
    // TODO: Name this
    RpcQueued,
    // RPC responses
    NewHeartbeat(i32),
    NewProcs(Vec<Process>),
    NewTrace(i32, Vec<Syscall>),
    NewMemInfo(i32, MemoryInfo),
    // Popup actions
    ClearError,
    // Sidebar actions
    ProcRateChange(u64),
    RequestProcs,
    OpenExec,
    // Exec window actions
    SetExec(String),
    ClearExec,
    RequestExec(String),
    // Process list actions
    SelectProcess(i32),
    DeleteProcess(i32),
    KillProcess(i32),
    // Tab actions
    TabSelected(TabId),
    // System call actions
    ChangeTraceMask(i32, u32),
    RefreshTrace(i32),
    // Memory actions
    RefreshMemory(i32),
}

enum Status {
    Connecting,
    Connected(mpsc::Sender<RpcReq>),
    Error(String),                                  // Fatal error
    RpcError(String, Option<mpsc::Sender<RpcReq>>), // Non-fatal error
}

struct App {
    // TODO: Organize these
    socket_path: String, // TODO: Remove or fix later
    status: Status,
    last_heartbeat: Option<i32>,
    proc_period: u64,
    processes: BTreeMap<i32, TracedProcess>,
    selected_process: Option<i32>,
    selected_tab: TabId,
    exec_modal: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        let socket_path =
            env::var("XDG_RUNTIME_DIR").unwrap_or(String::from("/tmp")) + "/xv6-serial0.sock";
        Self {
            socket_path,
            status: Status::Connecting,
            last_heartbeat: None,
            proc_period: 0, // 1000 ms default refresh rate, TODO: Switch back
            processes: BTreeMap::new(),
            selected_process: None,
            selected_tab: TabId::Syscalls,
            exec_modal: None,
        }
    }
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Exit => iced::exit(),
            Message::ExitError(err) => {
                self.status = Status::Error(err);
                Task::none()
            }
            Message::RpcError(err, keep_connected) => {
                let sender = if keep_connected {
                    match &self.status {
                        Status::Connected(s) => Some(s.clone()),
                        Status::RpcError(_, s) => s.clone(),
                        _ => None,
                    }
                } else {
                    None
                };
                self.status = Status::RpcError(err, sender);
                Task::none()
            }
            Message::ClearError => {
                if let Status::RpcError(_, Some(sender)) = &self.status {
                    self.status = Status::Connected(sender.clone());
                } else {
                    self.status = Status::Connecting;
                }
                Task::none()
            }
            Message::Connected(sender) => {
                self.status = Status::Connected(sender);
                Task::none()
            }
            Message::RequestMagic => self.send_rpc(RpcReq::Magic),
            Message::RequestHeartbeat => self.send_rpc(RpcReq::Heartbeat),
            Message::RequestProcs => self.send_rpc(RpcReq::PStat),
            Message::OpenExec => {
                self.exec_modal = Some(String::new());
                Task::none()
            }
            Message::SetExec(input) => {
                self.exec_modal = Some(input);
                Task::none()
            }
            Message::ClearExec => {
                self.exec_modal = None;
                Task::none()
            }
            Message::RequestExec(file) => {
                self.exec_modal = None;
                // TODO: Maybe add a delay between these (sometimes the UI shows the ichnos process forking before the exec() call)
                self.send_rpc(RpcReq::Exec(file))
                    .chain(self.send_rpc(RpcReq::PStat))
            }
            Message::RpcQueued => Task::none(),
            Message::NewHeartbeat(heartbeat) => {
                self.last_heartbeat = Some(heartbeat);
                Task::none()
            }
            Message::NewProcs(procs) => {
                self.update_processes(procs);
                Task::none()
            }
            Message::NewTrace(ref pid, events) => {
                // TODO: Maybe just unwrap here?
                if let Some(proc) = self.processes.get_mut(pid) {
                    proc.trace_events.extend(events);
                    println!(
                        "Updated trace events for PID {}: {:?}",
                        pid, proc.trace_events
                    );
                } else {
                    todo!() // TODO: Display an error here
                }
                Task::none()
            }
            Message::NewMemInfo(pid, meminfo) => {
                if let Some(proc) = self.processes.get_mut(&pid) {
                    proc.memory = Some(meminfo);
                } else {
                    todo!() // TODO: Display an error here
                }
                Task::none()
            }
            Message::KillProcess(pid) => self
                // TODO: Maybe add a delay between these
                .send_rpc(RpcReq::Kill(pid))
                .chain(self.send_rpc(RpcReq::PStat)),
            Message::ProcRateChange(rate) => {
                self.proc_period = rate;
                Task::none()
            }
            Message::SelectProcess(pid) => {
                if self.processes.contains_key(&pid) {
                    self.selected_process = Some(pid);
                } else {
                    // TODO: Throw an error or panic?
                    self.selected_process = None;
                }
                Task::none()
            }
            Message::DeleteProcess(pid) => {
                self.processes.remove(&pid);
                if self.selected_process == Some(pid) {
                    self.selected_process = None;
                }
                Task::none()
            }
            Message::TabSelected(tab_id) => {
                self.selected_tab = tab_id;
                Task::none()
            }
            Message::ChangeTraceMask(pid, mask) => {
                if let Some(proc) = self.processes.get_mut(&pid) {
                    proc.trace_mask = mask;
                    self.send_rpc(RpcReq::Trace { pid, mask })
                } else {
                    Task::none() // TODO: Throw an error or panic?
                }
            }
            Message::RefreshTrace(pid) => self.send_rpc(RpcReq::GetTrace(pid)),
            Message::RefreshMemory(pid) => self.send_rpc(RpcReq::MemInfo(pid)),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let sidebar = container(ui::sidebar(
            &self.proc_period,
            self.processes.values(),
            self.selected_process,
        ))
        .align_x(Alignment::Start)
        .width(Length::FillPortion(20))
        .align_top(Length::Fill);
        let main_pane: Element<'_, Message> = if let Some(selected_pid) = self.selected_process {
            // TODO: Use .icon_font() and the advanced text functions to set the advanced text modules of iced_fonts to fix the icons
            Tabs::new(Message::TabSelected)
                .push(
                    TabId::Syscalls,
                    TabLabel::IconText('\u{2699}', String::from("System Calls")),
                    ui::syscall_view(
                        self.processes
                            .get(&selected_pid)
                            .expect("Process should exist if selected"),
                    ),
                )
                .push(
                    TabId::Memory,
                    TabLabel::IconText('\u{1f4df}', String::from("Memory")),
                    ui::memory_view(
                        self.processes
                            .get(&selected_pid)
                            .expect("Process should exist if selected"),
                    ),
                )
                .set_active_tab(&self.selected_tab)
                .into()
        } else {
            center(text("Select a process to trace")).into()
        };
        let main_pane = container(main_pane).width(Length::FillPortion(80));
        let window = row![sidebar, main_pane];
        // TODO: Make container?

        match &self.status {
            Status::Error(err) => {
                let status_bar = ui::status_bar("Disconnected", false, self.last_heartbeat);
                let window = column![window, status_bar];
                let popup = ui::fatal_error_modal(err);
                ui::modal(window, popup, Message::Exit)
            }
            Status::RpcError(err, _) => {
                let status_bar = ui::status_bar("Disconnected", false, self.last_heartbeat);
                let window = column![window, status_bar];
                let popup = ui::error_modal(err);
                ui::modal(window, popup, Message::ClearError)
            }
            Status::Connecting => {
                let status_bar = ui::status_bar("Disconnected", false, self.last_heartbeat);
                column![window, status_bar].into()
            }
            Status::Connected(_) => {
                let status_bar = ui::status_bar(
                    format!("Connected to: {}", self.socket_path),
                    true,
                    self.last_heartbeat,
                );
                let window = column![window, status_bar];
                if let Some(exec_input) = &self.exec_modal {
                    let popup = ui::exec_modal(exec_input);
                    ui::modal(window, popup, Message::ClearExec)
                } else {
                    window.into()
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.proc_period > 0 {
            Subscription::batch(vec![
                Subscription::run(rpc_worker),
                time::every(std::time::Duration::from_millis(self.proc_period))
                    .map(|_| Message::RequestProcs),
            ])
        } else {
            Subscription::run(rpc_worker)
        }
    }

    fn send_rpc(&mut self, req: RpcReq) -> Task<Message> {
        if let Status::Connected(sender) = &mut self.status {
            let mut sender = sender.clone();
            Task::perform(async move { sender.send(req).await }, |res| match res {
                Ok(_) => Message::RpcQueued,
                Err(err) => {
                    Message::RpcError(format!("Failed to send RPC request: {}", err), false)
                }
            })
        } else {
            Task::none()
        }
    }

    // TODO: Fix these comments
    fn update_processes(&mut self, procs: Vec<Process>) {
        let mut received_pids = HashSet::new();

        for proc in procs {
            let pid = proc.pid;
            received_pids.insert(pid);

            use std::collections::btree_map::Entry;

            match self.processes.entry(pid) {
                Entry::Occupied(mut entry) => {
                    let tracked = entry.get_mut();
                    if !tracked.is_alive {
                        // TODO: Maybe panic here?
                        self.status = Status::Error(format!(
                            "Ghost process detected: PID {} was dead but appeared again.",
                            pid
                        ));
                        return;
                    }
                    tracked.process = proc;
                }
                Entry::Vacant(entry) => {
                    entry.insert(TracedProcess::new(proc));
                }
            }
        }
        // Mark processes not currently received as dead
        for (pid, tracked) in self.processes.iter_mut() {
            if !received_pids.contains(pid) {
                tracked.is_alive = false;
            }
        }
    }
}

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .subscription(App::subscription)
        .font(ICED_AW_FONT_BYTES)
        .font(CODICON_FONT_BYTES)
        .run()
}

async fn connect_rpc_socket(
    socket_path: &str,
    output: &mut mpsc::Sender<Message>,
) -> Option<UnixStream> {
    let mut attempt = 0;

    loop {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return Some(stream),
            Err(err) => {
                attempt += 1;

                if attempt >= RPC_CONNECT_MAX_ATTEMPTS {
                    output
                        .send(Message::ExitError(format!(
                            "Failed to connect to xv6 after {} attempts.\nError: {}\nSocket Path: {}",
                            attempt,
                            err,
                            socket_path
                        )))
                        .await
                        .unwrap();
                    return None;
                }

                output
                    .send(Message::RpcError(format!(
                        "Failed to connect to xv6: {}\nRetrying in {} seconds... (Attempt {}/{})",
                        err,
                        RPC_CONNECT_RETRY_DELAY.as_secs(),
                        attempt,
                        RPC_CONNECT_MAX_ATTEMPTS,
                    ), false))
                    .await
                    .unwrap();

                sleep(RPC_CONNECT_RETRY_DELAY).await;
            }
        }
    }
}

fn rpc_worker() -> impl Stream<Item = Message> {
    stream::channel(16, async |mut output| {
        let socket_path =
            env::var("XDG_RUNTIME_DIR").unwrap_or(String::from("/tmp")) + "/xv6-serial0.sock";

        let stream = match connect_rpc_socket(&socket_path, &mut output).await {
            Some(stream) => stream,
            None => return,
        };

        let (sender, mut receiver) = mpsc::channel(16);
        output
            .send(Message::Connected(sender.clone()))
            .await
            .unwrap(); // TODO: Confirm unwrap correctness

        let mut rpc_handler = RpcHandler::new(stream);
        while let Some(req) = receiver.next().await {
            // TODO: Maybe add a retry mechanism here?
            match rpc_handler.send_request(req.clone()).await {
                Ok(resp) => {
                    match resp {
                        RpcResp::Magic(magic) => {
                            if magic != MAGIC {
                                output
                                    .send(Message::RpcError(
                                        format!(
                                            "Invalid magic number 0x{:X} (expected: 0x{:X})",
                                            magic, MAGIC
                                        ),
                                        true,
                                    ))
                                    .await
                                    .unwrap();
                            }
                        }
                        RpcResp::Heartbeat(ticks) => {
                            output.send(Message::NewHeartbeat(ticks)).await.unwrap()
                        }
                        RpcResp::PStat(procs) => {
                            output.send(Message::NewProcs(procs)).await.unwrap()
                        }
                        RpcResp::Kill(success) => {
                            // Kill triggers a refresh
                            println!("Kill: {}", success); // TODO: Do smth here, i.e. if kill fails (kind of tricky)
                            output.send(Message::RequestProcs).await.unwrap(); // TODO: Maybe not do this here?
                        }
                        RpcResp::Trace(success) => println!("Trace: {}", success), // TODO: Do something here, deal with errors
                        RpcResp::GetTrace(events) => output
                            .send(Message::NewTrace(req.get_pid(), events))
                            .await
                            .unwrap(),
                        RpcResp::Exec(pid) => println!("Exec: {}", pid), // TODO: Do something here, deal with errors
                        RpcResp::MemInfo(resp) => {
                            if let Some(meminfo) = resp {
                                output
                                    .send(Message::NewMemInfo(req.get_pid(), meminfo))
                                    .await
                                    .unwrap();
                            } else {
                                output
                                    .send(Message::RpcError(
                                        format!(
                                            "Failed to get memory info for PID {}",
                                            req.get_pid()
                                        ),
                                        true,
                                    ))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
                Err(err) => {
                    output
                        .send(Message::RpcError(
                            format!("RPC failed: {}", err),
                            err.kind() == std::io::ErrorKind::InvalidData,
                        ))
                        .await
                        .unwrap();

                    // TODO: Validate this logic
                    // Only reconnect if it's a fatal stream error, not an invalid data error.
                    if err.kind() != std::io::ErrorKind::InvalidData {
                        if let Some(stream) = connect_rpc_socket(&socket_path, &mut output).await {
                            rpc_handler = RpcHandler::new(stream);
                            output
                                .send(Message::Connected(sender.clone()))
                                .await
                                .unwrap();
                        } else {
                            return;
                        }
                    }
                }
            }
        }
        // TODO: Maybe through an error here?
    })
}
