use dioxus::prelude::*;
use futures_timer::Delay;
use std::time::Duration;

const STATUS_AUTO_CLEAR_DELAY: Duration = Duration::from_secs(4);
const SEARCH_DEBOUNCE_DELAY: Duration = Duration::from_millis(120);

/// Hook to auto-clear status messages after a delay
pub fn use_status_auto_clear_effect(
    status: Signal<String>,
    mut status_clear_generation: Signal<u32>,
) {
    use_effect(move || {
        let message = status();
        if message.is_empty() {
            return;
        }

        let generation = *status_clear_generation.peek() + 1;
        status_clear_generation.set(generation);
        spawn(async move {
            Delay::new(STATUS_AUTO_CLEAR_DELAY).await;
            if *status_clear_generation.peek() == generation
                && status.peek().as_str() == message.as_str()
            {
                let mut status = status;
                status.set(String::new());
            }
        });
    });
}

/// Hook to debounce search query input
pub fn use_search_debounce_effect(
    query: Signal<String>,
    mut debounced_query: Signal<String>,
    mut search_generation: Signal<u32>,
) {
    use_effect(move || {
        let next_query = query();
        let generation = *search_generation.peek() + 1;
        search_generation.set(generation);

        if next_query.is_empty() {
            debounced_query.set(String::new());
            return;
        }

        spawn(async move {
            Delay::new(SEARCH_DEBOUNCE_DELAY).await;
            if *search_generation.peek() == generation {
                debounced_query.set(next_query);
            }
        });
    });
}
