//! Handle the commands sent from other threads.

use super::{Command, Entry, STOP_GRACE, Worker, reset_for_start};
use crate::child::stop_all;

impl Worker {
    pub(super) fn handle(&mut self, cmd: Command) {
        self.debug(&format!("command: {cmd:?}"));
        match cmd {
            Command::Restart(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = false;
                    let proc = e.proc.take();
                    reset_for_start(e, &self.config);
                    stop_all(proc.into_iter().collect(), STOP_GRACE);
                }
            }
            Command::Stop(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = true;
                    e.next_start = None;
                    e.next_start_unix = None;
                    stop_all(e.proc.take().into_iter().collect(), STOP_GRACE);
                }
            }
            Command::Start(key) => {
                if let Some(e) = self.entries.get_mut(&key) {
                    e.manual_stop = false;
                    if e.proc.is_none() {
                        reset_for_start(e, &self.config);
                    }
                }
            }
            Command::RestartAll => {
                let mut procs = Vec::new();
                for e in self.entries.values_mut() {
                    e.manual_stop = false;
                    procs.extend(e.proc.take());
                    reset_for_start(e, &self.config);
                }
                stop_all(procs, STOP_GRACE);
            }
            Command::Reload => {
                self.reload_config();
                self.last_scan = None;
            }
            Command::Answer { key, text } => {
                if let Some(e) = self.entries.get(&key) {
                    answer(e, &text);
                    self.log(&format!("{}: answered {text:?}", e.target.name));
                }
            }
            Command::AnswerAll { attention, text } => {
                let names: Vec<String> = self
                    .entries
                    .values()
                    .filter(|e| e.output.lock().unwrap().attention == Some(attention))
                    .map(|e| {
                        answer(e, &text);
                        e.target.name.clone()
                    })
                    .collect();
                if !names.is_empty() {
                    self.log(&format!("answered {text:?} for {}", names.join(", ")));
                }
            }
            Command::Shutdown => {}
        }
    }
}

fn answer(e: &Entry, text: &str) {
    if let Some(p) = &e.proc {
        p.write_line(text);
        e.output.lock().unwrap().attention = None;
    }
}
