// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// One device heard, as a window reports it.
fn seen(udid: &str) -> ScanEntry {
    ScanEntry {
        unit: udid.to_string(),
        product: Some("Gungnir 2".into()),
        product_id: 2,
        firmware: 3,
        passphrase: false,
        rssi: Some(-50),
    }
}

/// What one window of a graded adapter answers.
#[derive(Clone)]
enum Heard {
    /// The device was on the air.
    Device,
    /// The window was quiet.
    Nothing,
    /// The window itself did not run, for the reason it gives.
    Failed(String),
}

/// A scripted source: a list of windows to answer a search with, or, per adapter, what each
/// of its windows answers.
///
/// Every window takes the time a real one would, so a test on a paused clock sees the
/// same order of events a real search would and none of the waiting.
struct FakeScans {
    /// What each window of a search answers, in order; a search past the end hears
    /// nothing.
    windows: Vec<Vec<ScanEntry>>,
    /// What each of one adapter's windows answers; a window past the end is quiet.
    per_adapter: BTreeMap<String, Vec<Heard>>,
    /// How many windows have run, over every adapter.
    runs: AtomicUsize,
    /// How many windows have run on each adapter.
    runs_on: Mutex<BTreeMap<String, usize>>,
}

impl FakeScans {
    /// A source that answers a search with `windows`, one after another.
    fn new(windows: Vec<Vec<ScanEntry>>) -> FakeScans {
        FakeScans {
            windows,
            per_adapter: BTreeMap::new(),
            runs: AtomicUsize::new(0),
            runs_on: Mutex::new(BTreeMap::new()),
        }
    }

    /// A source where each named adapter hears the device in the windows the flags mark.
    fn per_adapter<const N: usize>(adapters: [(&str, Vec<bool>); N]) -> FakeScans {
        let scripts = adapters.map(|(name, windows)| {
            let heard = windows
                .into_iter()
                .map(|hears| match hears {
                    true => Heard::Device,
                    false => Heard::Nothing,
                })
                .collect();
            (name, heard)
        });
        FakeScans::scripted(scripts)
    }

    /// A source where each named adapter's windows answer as the script says.
    fn scripted<const N: usize>(adapters: [(&str, Vec<Heard>); N]) -> FakeScans {
        let mut source = FakeScans::new(Vec::new());
        source.per_adapter = adapters
            .into_iter()
            .map(|(name, windows)| (name.to_string(), windows))
            .collect();
        source
    }

    /// A source whose only adapter will not open, so every window of it fails.
    fn refusing(adapter: &str, why: &str) -> FakeScans {
        let windows = vec![Heard::Failed(why.to_string()); GRADE_WINDOWS as usize];
        FakeScans::scripted([(adapter, windows)])
    }

    /// How many windows have run.
    fn windows_run(&self) -> usize {
        self.runs.load(Ordering::SeqCst)
    }
}

#[async_trait(?Send)]
impl ScanSource for FakeScans {
    async fn window(
        &self,
        adapter: Option<&str>,
        want: Option<&str>,
    ) -> Result<Vec<ScanEntry>, Failure> {
        let Some(name) = adapter else {
            let at = self.runs.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(WINDOW).await;
            return Ok(self.windows.get(at).cloned().unwrap_or_default());
        };
        let at = {
            let mut runs = self.runs_on.lock().expect("the run counter");
            let at = runs.entry(name.to_string()).or_default();
            let now = *at;
            *at += 1;
            now
        };
        self.runs.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(GRADE_WINDOW).await;
        let heard = self
            .per_adapter
            .get(name)
            .and_then(|windows| windows.get(at))
            .cloned()
            .unwrap_or(Heard::Nothing);
        match (heard, want) {
            (Heard::Device, Some(udid)) => Ok(vec![seen(udid)]),
            (Heard::Failed(why), _) => Err(Failure::of(crate::recovery::Kind::AdapterMissing, why)),
            _ => Ok(Vec::new()),
        }
    }
}

/// A search for a device stops at the window that hears it, and gives up after three.
#[tokio::test(start_paused = true)]
async fn find_stops_at_the_first_window_that_sees_the_device_and_gives_up_after_three() {
    let source = FakeScans::new(vec![vec![], vec![seen("0102030405060708")]]);
    let found = find_with(&source, Some("0102030405060708"))
        .await
        .expect("the second window hears it");
    assert_eq!(source.windows_run(), 2);
    assert_eq!(found.len(), 1);

    let none = FakeScans::new(vec![vec![], vec![], vec![]]);
    let err = find_with(&none, Some("0102030405060708"))
        .await
        .expect_err("three quiet windows");
    assert_eq!(none.windows_run(), WINDOWS as usize);
    assert!(err.what.contains("3 windows"), "{}", err.what);
    assert!(err.next.contains("phone app"), "{}", err.next);
}

/// A scan that names no device is the one window it always was.
#[tokio::test(start_paused = true)]
async fn a_search_for_nothing_in_particular_runs_one_window() {
    let source = FakeScans::new(vec![vec![seen("0102030405060708")], vec![]]);
    let found = find_with(&source, None).await.expect("one window");
    assert_eq!(source.windows_run(), 1);
    assert_eq!(found.len(), 1);
}

/// A search for whatever is out there on the wizard's behalf runs windows until one hears
/// something, and gives up quietly after three rather than failing: a run that heard
/// nothing is what the wizard offers a rescan under.
#[tokio::test(start_paused = true)]
async fn a_guided_search_runs_windows_until_one_hears_something() {
    let source = FakeScans::new(vec![vec![], vec![seen("0102030405060708")]]);
    let found = find_any_with(&source).await.expect("the second window");
    assert_eq!(source.windows_run(), 2);
    assert_eq!(found.len(), 1);

    let none = FakeScans::new(vec![vec![], vec![], vec![]]);
    let quiet = find_any_with(&none).await.expect("three quiet windows");
    assert_eq!(none.windows_run(), WINDOWS as usize);
    assert!(quiet.is_empty());
}

/// Grading counts the windows each adapter caught the device in, adapter by adapter.
#[tokio::test(start_paused = true)]
async fn grading_counts_the_windows_each_adapter_caught_the_device_in() {
    let source = FakeScans::per_adapter([
        ("hci0", vec![true, false, true, false, false]),
        ("hci1", vec![true; 5]),
    ]);
    let grades = grade_with(&source, "0102030405060708", &["hci0".into(), "hci1".into()]).await;
    assert_eq!(
        grades
            .iter()
            .map(|g| g.adapter.as_str())
            .collect::<Vec<_>>(),
        vec!["hci0", "hci1"]
    );
    assert_eq!((grades[0].seen, grades[0].of), (2, 5));
    assert_eq!((grades[1].seen, grades[1].of), (5, 5));
    assert_eq!(grades[0].note, None);
}

/// An adapter that will not open hears nothing, and its grade says why: the first window
/// is the one that opens it, and there is nothing left to grade after that.
#[tokio::test(start_paused = true)]
async fn an_adapter_that_will_not_open_grades_nothing_and_says_why() {
    let source = FakeScans::refusing("hci9", "no adapter named hci9");
    let grades = grade_with(&source, "0102030405060708", &["hci9".into()]).await;
    assert_eq!((grades[0].seen, grades[0].of), (0, GRADE_WINDOWS));
    assert_eq!(grades[0].note.as_deref(), Some("no adapter named hci9"));
    assert_eq!(source.windows_run(), 1, "the adapter was tried once");
}

/// A window that fails once the adapter is open is that window lost, not a verdict on the
/// adapter: it counts as a miss, the grading runs on, and the note says what happened.
#[tokio::test(start_paused = true)]
async fn a_window_that_fails_mid_run_is_a_miss_with_a_reason() {
    let source = FakeScans::scripted([(
        "hci0",
        vec![
            Heard::Device,
            Heard::Device,
            Heard::Failed("the scan stopped".into()),
            Heard::Nothing,
            Heard::Nothing,
        ],
    )]);
    let grades = grade_with(&source, "0102030405060708", &["hci0".into()]).await;
    assert_eq!((grades[0].seen, grades[0].of), (2, GRADE_WINDOWS));
    assert_eq!(grades[0].note.as_deref(), Some("the scan stopped"));
    assert_eq!(
        source.windows_run(),
        GRADE_WINDOWS as usize,
        "every window ran"
    );
}
