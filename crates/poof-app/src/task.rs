//! Running blocking work (disk scans, deletions, shell commands) without
//! freezing the UI.

use iced::Task;
use iced::futures::channel::oneshot;

/// Runs `work` on a dedicated thread and maps its result to a message.
pub fn blocking<T, M>(
    work: impl FnOnce() -> T + Send + 'static,
    to_message: impl FnOnce(T) -> M + Send + 'static,
) -> Task<M>
where
    T: Send + 'static,
    M: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        // The receiver only disappears if the app is shutting down.
        let _ = sender.send(work());
    });
    Task::perform(async move { receiver.await.expect("background worker panicked") }, to_message)
}
