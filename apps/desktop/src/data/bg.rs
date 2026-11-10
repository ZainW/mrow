//! Work off the UI thread: a job runs on its own thread and sends back a message, which `poll`
//! lands on the next frame. Nothing polls in between; the waker asks for that frame.

use std::sync::mpsc::{Receiver, Sender, channel};

use rune::Waker;

pub struct Bg<M> {
    tx: Sender<M>,
    rx: Receiver<M>,
    waker: Option<Waker>,
}

impl<M: Send + 'static> Default for Bg<M> {
    fn default() -> Self {
        let (tx, rx) = channel();
        Bg { tx, rx, waker: None }
    }
}

impl<M: Send + 'static> Bg<M> {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    pub fn spawn(&self, job: impl FnOnce() -> M + Send + 'static) {
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            if let Some(w) = waker {
                w();
            }
        });
    }

    pub fn drain(&self) -> Vec<M> {
        self.rx.try_iter().collect()
    }
}
