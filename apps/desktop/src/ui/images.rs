//! Pictures in markdown: asked for while the view is drawn, fetched on background threads a few
//! at a time, and decoded by Rune's image cache.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use rune::{Image, ImageCache, Waker};

use crate::app::App;
use crate::data::images::ImageSource;

/// At most this many downloads at once.
const PARALLEL: usize = 4;

#[derive(Default)]
struct Shared {
    /// Decoded (or decoding) pictures by source key.
    loaded: HashMap<String, Image>,
    /// Sources that failed, so they aren't asked for again.
    failed: HashSet<String>,
    /// Asked for by the view, not yet fetching.
    wanted: VecDeque<ImageSource>,
    queued: HashSet<String>,
    /// Wakes the frame loop so `poll` starts what the view just asked for.
    waker: Option<Waker>,
}

pub struct Images {
    shared: Rc<RefCell<Shared>>,
    cache: Option<ImageCache>,
    waker: Option<Waker>,
    tx: Sender<(String, Option<Vec<u8>>)>,
    rx: Receiver<(String, Option<Vec<u8>>)>,
    active: usize,
    /// Snapshots and benchmarks never touch the network.
    pub offline: bool,
}

impl Default for Images {
    fn default() -> Self {
        let (tx, rx) = channel();
        Images { shared: Rc::default(), cache: None, waker: None, tx, rx, active: 0, offline: false }
    }
}

impl Images {
    pub fn connect(&mut self, waker: Waker) {
        self.cache = Some(ImageCache::new(waker.clone()));
        self.shared.borrow_mut().waker = Some(waker.clone());
        self.waker = Some(waker);
    }

    /// Lands finished downloads and starts the next ones. True when a picture arrived.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok((key, bytes)) = self.rx.try_recv() {
            self.active -= 1;
            let mut shared = self.shared.borrow_mut();
            match (bytes, &self.cache) {
                (Some(bytes), Some(cache)) => {
                    shared.loaded.insert(key, cache.load(bytes));
                    changed = true;
                }
                _ => {
                    shared.failed.insert(key);
                }
            }
        }
        if self.offline || self.cache.is_none() {
            return changed;
        }
        while self.active < PARALLEL {
            let Some(source) = self.shared.borrow_mut().wanted.pop_front() else { break };
            self.active += 1;
            let tx = self.tx.clone();
            let waker = self.waker.clone();
            std::thread::spawn(move || {
                let bytes = source.fetch();
                let _ = tx.send((source.key(), bytes));
                if let Some(w) = waker {
                    w();
                }
            });
        }
        changed
    }
}

/// What the markdown view calls for each picture: the image once it's there, asking for it
/// the first time.
pub fn lookup(s: &App) -> impl Fn(&str, Option<&str>) -> Option<Image> + 'static {
    let shared = s.images.shared.clone();
    move |src, base| {
        let source = ImageSource::resolve(src, base)?;
        let key = source.key();
        let mut shared = shared.borrow_mut();
        if let Some(image) = shared.loaded.get(&key) {
            return Some(image.clone());
        }
        if !shared.failed.contains(&key) && shared.queued.insert(key) {
            shared.wanted.push_back(source);
            if let Some(w) = &shared.waker {
                w();
            }
        }
        None
    }
}
