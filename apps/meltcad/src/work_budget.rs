//! One high-memory job per document window. The permit belongs to the worker, not the card:
 //! closing a dialog or requesting cancellation cannot start another large allocation until
 //! the previous worker has really stopped.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const BUSY: &str = "ほかの重い処理が終わるまでお待ちください（メモリ保護のため同時実行できません）";

#[derive(Clone, Default)]
struct Budget(Arc<AtomicBool>);

impl Budget {
    fn try_acquire(&self) -> Result<Permit, String> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BUSY.to_string())?;
        Ok(Permit { busy: self.0.clone() })
    }
}

/// RAII guard held until the actual worker exits, even if it panics.
pub struct Permit {
    busy: Arc<AtomicBool>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}

fn id() -> egui::Id {
    egui::Id::new("yura_high_memory_job_budget")
}

/// Is a high-memory worker still active? Closing a card does not release its permit.
pub fn is_busy(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Budget>(id()))
        .is_some_and(|b| b.0.load(Ordering::Acquire))
}

/// Context-scoped, so two independent document windows/tests do not block each other.
pub fn try_acquire(ctx: &egui::Context) -> Result<Permit, String> {
    let budget = ctx.data_mut(|data| {
        let budget = data.get_temp::<Budget>(id()).unwrap_or_default();
        data.insert_temp(id(), budget.clone());
        budget
    });
    budget.try_acquire()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_large_job_waits_for_the_worker_not_the_dialog() {
        let ctx = egui::Context::default();
        let guard = try_acquire(&ctx).expect("first worker");
        assert_eq!(try_acquire(&ctx).err().as_deref(), Some(BUSY));
        drop(guard);
        assert!(try_acquire(&ctx).is_ok(), "the worker exited and freed its permit");
    }

    #[test]
    fn budgets_belong_to_independent_window_contexts() {
        let a = egui::Context::default();
        let b = egui::Context::default();
        let _one = try_acquire(&a).expect("window a");
        assert!(try_acquire(&b).is_ok(), "independent contexts should not share a process-wide lock");
    }

    #[test]
    fn a_panicking_worker_releases_the_permit() {
        let ctx = egui::Context::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _permit = try_acquire(&ctx).expect("permit");
            panic!("test failure");
        }));
        assert!(result.is_err());
        assert!(try_acquire(&ctx).is_ok());
    }
}
