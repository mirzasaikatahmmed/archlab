//! One background job of a kind at a time, with a cancel flag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use super::RuntimeError;

#[derive(Default)]
pub struct JobSlot {
    active: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl JobSlot {
    /// Runs `job` on its own thread. It receives the cancel flag and should poll it.
    pub fn spawn(
        &self,
        name: &'static str,
        job: impl FnOnce(&AtomicBool) + Send + 'static,
    ) -> Result<(), RuntimeError> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut slot = self.active.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_some() {
                return Err(RuntimeError::Busy(name));
            }
            *slot = Some(Arc::clone(&cancel));
        }
        let slot = Arc::clone(&self.active);
        let spawned = thread::Builder::new()
            .name(format!("archlab-{}", name.replace(' ', "-")))
            .spawn(move || {
                job(&cancel);
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
            });
        if let Err(err) = spawned {
            *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
            return Err(RuntimeError::Failed(format!(
                "could not start {name}: {err}"
            )));
        }
        Ok(())
    }

    pub fn cancel(&self, name: &'static str) -> Result<(), RuntimeError> {
        let slot = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let flag = slot.as_ref().ok_or(RuntimeError::NothingToCancel(name))?;
        flag.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    fn wait_idle(slot: &JobSlot) {
        let end = Instant::now() + Duration::from_secs(5);
        while slot.cancel("job").is_ok() {
            assert!(Instant::now() < end, "job did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn runs_one_job_at_a_time_and_frees_the_slot() {
        let slot = JobSlot::default();
        let (tx, rx) = mpsc::channel();
        slot.spawn("job", move |cancel| {
            while !cancel.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(2));
            }
            tx.send(()).expect("send");
        })
        .expect("first");
        assert!(matches!(
            slot.spawn("job", |_| {}),
            Err(RuntimeError::Busy("job"))
        ));
        slot.cancel("job").expect("cancel");
        rx.recv_timeout(Duration::from_secs(5)).expect("finished");
        wait_idle(&slot);
        assert!(slot.spawn("job", |_| {}).is_ok());
    }

    #[test]
    fn cancelling_with_nothing_running_is_an_error() {
        let slot = JobSlot::default();
        assert!(matches!(
            slot.cancel("job"),
            Err(RuntimeError::NothingToCancel("job"))
        ));
    }
}
