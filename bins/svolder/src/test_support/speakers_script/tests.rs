// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scripted sets service's waits end: a case whose job never comes fails, never hangs.

use super::SpeakersScript;

#[tokio::test(start_paused = true)]
#[should_panic(expected = "a job was made on the stub daemon within 5 s")]
async fn waiting_for_a_job_that_is_never_made_fails() {
    SpeakersScript::default().job_started().await;
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "a cancel arrived at the stub daemon within 5 s")]
async fn waiting_for_a_cancel_that_never_comes_fails() {
    SpeakersScript::default().cancel_arrived().await;
}
