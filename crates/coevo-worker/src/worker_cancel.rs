use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct RunCancellationToken {
    token: CancellationToken,
}

impl RunCancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

pub struct RunCancellationRegistration {
    run_id: String,
    token: CancellationToken,
}

impl RunCancellationRegistration {
    pub fn token(&self) -> RunCancellationToken {
        RunCancellationToken {
            token: self.token.clone(),
        }
    }

    pub fn cancel(&self) -> bool {
        self.token.cancel();
        true
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

impl Drop for RunCancellationRegistration {
    fn drop(&mut self) {
        if let Some(registry) = registry().get() {
            if let Ok(mut guard) = registry.lock() {
                guard.remove(&self.run_id);
            }
        }
    }
}

static REGISTRY: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();

fn registry() -> &'static OnceLock<Mutex<HashMap<String, CancellationToken>>> {
    &REGISTRY
}

fn registry_lock() -> &'static Mutex<HashMap<String, CancellationToken>> {
    registry().get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_run(run_id: impl Into<String>) -> RunCancellationRegistration {
    let run_id = run_id.into();
    let token = CancellationToken::new();
    if let Ok(mut guard) = registry_lock().lock() {
        guard.insert(run_id.clone(), token.clone());
    }
    RunCancellationRegistration { run_id, token }
}

pub fn token_for_run(run_id: &str) -> Option<RunCancellationToken> {
    registry_lock()
        .lock()
        .ok()
        .and_then(|guard| guard.get(run_id).cloned())
        .map(|token| RunCancellationToken { token })
}

pub fn cancel_run(run_id: &str) -> bool {
    let token = registry_lock()
        .lock()
        .ok()
        .and_then(|guard| guard.get(run_id).cloned());
    match token {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

pub fn is_run_cancelled(run_id: &str) -> bool {
    token_for_run(run_id)
        .map(|token| token.is_cancelled())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_state_is_visible_to_late_subscribers() {
        let run_id = format!("run-cancel-late-subscriber-{}", uuid::Uuid::new_v4());
        let _registration = register_run(run_id.clone());

        assert!(cancel_run(&run_id));

        let token = token_for_run(&run_id).expect("token should still exist after cancellation");
        assert!(token.is_cancelled());
        assert!(is_run_cancelled(&run_id));
    }
}
