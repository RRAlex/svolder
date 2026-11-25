// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A relative step and then a glide to 99 wait behind a write whose answer is the timeout, after
/// the volume was read at 111. The knob is turned down to 51 while they wait, so the volume reads
/// 52 once the step is judged: the step goes out as 52 (the reading plus one), and the glide,
/// judged a lowering from 111 when it arrived, is judged again as it goes out, so it writes the
/// step the rule lets land (95), never 99 over the volume the knob left.
#[tokio::test(start_paused = true)]
async fn a_glide_that_waited_is_judged_again_from_the_reading_when_it_goes_out() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 111)]);
    stand
        .times_out
        .store(true, std::sync::atomic::Ordering::SeqCst);
    stand.gate.forget_permits(1000);
    let stuck = writer.submit(Origin::Socket, set("volume", "111"), Mode::Ordered);
    let glided = glide(&writer, Origin::Mcp, 99);
    let step = writer.submit(Origin::Socket, set("volume", "up"), Mode::Ordered);
    let turned = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        turn(&stand, 51);
        stand.gate.add_permits(1000);
    };
    let (stuck, glided, step, ()) = tokio::join!(stuck, glided, step, turned);
    assert!(matches!(stuck, Ok(Outcome::Done(_))), "{stuck:?}");
    assert!(matches!(glided, Glided::Landed(_)), "{glided:?}");
    assert!(matches!(step, Ok(Outcome::Done(_))), "{step:?}");
    assert_eq!(volume_steps(&stand), [111, 52, 95]);
}

/// The same wait in the other order: an ordered glide to 99, then a relative step, both behind
/// the stuck write, the volume read at 111 and the knob turned down to 51 just before the next
/// command goes out. The glide is judged again as it goes out and writes the step the rule lets
/// land (95), and the step after it follows from that, so no write is louder than the rule allows
/// from the volume the knob left.
#[tokio::test(start_paused = true)]
async fn an_ordered_glide_then_a_step_that_waited_never_write_over_the_knob() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 111)]);
    stand
        .times_out
        .store(true, std::sync::atomic::Ordering::SeqCst);
    stand.gate.forget_permits(1000);
    let stuck = writer.submit(Origin::Socket, set("volume", "111"), Mode::Ordered);
    let queued = async {
        let glided = writer.glide(Origin::Mcp, "volume".into(), 99, Mode::Ordered);
        let step = async {
            writer
                .submit(Origin::Socket, set("volume", "up"), Mode::Ordered)
                .await
        };
        tokio::join!(glided, step)
    };
    let turned = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        turn(&stand, 51);
        stand.gate.add_permits(1000);
    };
    let (stuck, (glided, step), ()) = tokio::join!(stuck, queued, turned);
    assert!(matches!(stuck, Ok(Outcome::Done(_))), "{stuck:?}");
    assert!(matches!(glided, Ok(Glided::Landed(_))), "{glided:?}");
    assert!(matches!(step, Ok(Outcome::Done(_))), "{step:?}");
    assert_eq!(volume_steps(&stand), [111, 95, 96]);
}
