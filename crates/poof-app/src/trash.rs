//! Emptying the Trash, offered right after Poof moves something there so the
//! space can be reclaimed without leaving the app.

use poof_core::remove;

use crate::screen::{Action, Event};
use crate::style::Tone;
use crate::task;
use crate::widget::Dialog;

#[derive(Debug, Default)]
pub struct State {
    confirming: bool,
    emptying: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Ask,
    Cancel,
    Confirm,
    /// Bytes freed and items that couldn't be deleted, or what went wrong.
    Emptied(Result<(u64, usize), String>),
}

impl State {
    pub fn update(&mut self, message: Message) -> Action<Message> {
        match message {
            Message::Ask => {
                self.confirming = !self.emptying;
                Action::none()
            }
            Message::Cancel => {
                self.confirming = false;
                Action::none()
            }
            Message::Confirm => {
                self.confirming = false;
                self.emptying = true;
                task::blocking(
                    || {
                        remove::empty_trash().map(|o| (o.bytes, o.failed.len())).map_err(|e| {
                            if e.is_permission_denied() {
                                "Poof needs Full Disk Access to empty the Trash. \
                                 You can also empty it from the Dock."
                                    .to_owned()
                            } else {
                                format!("Couldn't empty the Trash: {e}.")
                            }
                        })
                    },
                    Message::Emptied,
                )
                .into()
            }
            Message::Emptied(result) => {
                self.emptying = false;
                Action::event(match result {
                    Ok((0, 0)) => Event::Toast("The Trash is already empty.".into(), Tone::Success),
                    Ok((bytes, failed)) => Event::Removed { bytes, trashed: false, failed },
                    Err(message) => Event::Toast(message, Tone::Warning),
                })
            }
        }
    }

    pub fn dialog(&self) -> Option<Dialog<Message>> {
        self.confirming.then(|| Dialog {
            title: "Empty the Trash?".into(),
            message: "Everything in the Trash is deleted permanently. You can't undo this.".into(),
            details: Vec::new(),
            confirm: "Empty Trash".into(),
            destructive: true,
            on_confirm: Message::Confirm,
            on_cancel: Message::Cancel,
        })
    }
}
