//! Regression tests from the pre-launch code review (each first written to show a defect), kept now
//! that the defects are fixed.
use std::path::{Path, PathBuf};
use std::time::Duration;

use onkey_lib::store::write_atomic;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("onkey-review-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const BIG: usize = 8 << 20;

/// Not a test on its own: the writer that `a_write_killed_midway_leaves_the_old_file` starts and kills. Writes a
/// big file over and over (all one byte, a different byte each time) until killed; atomically, or with a plain
/// truncate-and-write when ONKEY_PROBE_PLAIN is set.
#[test]
fn probe_writer_child() {
    let Ok(path) = std::env::var("ONKEY_PROBE_WRITE") else { return };
    let path = PathBuf::from(path);
    let plain = std::env::var("ONKEY_PROBE_PLAIN").is_ok();
    let bufs: Vec<Vec<u8>> = (0..4u8).map(|k| vec![b'b' + k; BIG]).collect();
    for k in 0.. {
        let b = &bufs[k % bufs.len()];
        if plain { std::fs::write(&path, b).unwrap() } else { write_atomic(&path, b).unwrap() }
    }
}

/// Kill a writer `rounds` times at different moments mid-way; how many times the file was left damaged (not
/// whole, or a mix of two writes).
fn kill_writer_midway(path: &Path, plain: bool, rounds: u64) -> usize {
    let exe = std::env::current_exe().unwrap();
    let mut damaged = 0;
    for round in 0..rounds {
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(["probe_writer_child", "--exact", "--nocapture", "--test-threads=1"])
            .env("ONKEY_PROBE_WRITE", path)
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        if plain {
            cmd.env("ONKEY_PROBE_PLAIN", "1");
        }
        let mut child = cmd.spawn().unwrap();
        std::thread::sleep(Duration::from_millis(250 + 23 * round));
        child.kill().unwrap();                     // SIGKILL: no clean-up, as a crash or `kill -9`
        child.wait().unwrap();
        let b = std::fs::read(path).unwrap_or_default();
        if b.len() != BIG || b.iter().any(|x| *x != b[0]) {
            damaged += 1;
        }
    }
    damaged
}

/// Finding 2: every file was written with a plain truncate-and-write, so a kill mid-write left it cut short (a
/// song vanished from the Library, settings.json lost control and setup_done). Now each write goes to a temp file
/// and is renamed over the old one: killed at any moment, the file is the old one or the new one, whole.
#[test]
fn a_write_killed_midway_leaves_the_old_file() {
    let dir = tmp("atomic");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    write_atomic(&path, &vec![b'a'; BIG]).unwrap();
    let damaged = kill_writer_midway(&path, false, 12);
    // the same kills with the old way of writing, to show the test can tell
    let plain = dir.join("plain.json");
    std::fs::write(&plain, vec![b'a'; BIG]).unwrap();
    let damaged_plain = kill_writer_midway(&plain, true, 12);
    println!("killed mid-write 12 times: atomic writes left {damaged} damaged files, plain writes {damaged_plain}");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(damaged, 0, "an atomic write killed midway must leave the old file or the new one, whole");
}

/// Finding 4: store::take_dir accepted "." and "..", so delete_take("..") resolved to the app data folder and,
/// when the move to the Trash failed, removed all of it. Now only take ids are accepted, by every command.
#[test]
fn take_dir_refuses_anything_but_a_take_id() {
    use onkey_lib::store::{valid_take_id, Store};
    let dir = tmp("takedir");
    let store = Store::new(dir.clone());
    std::fs::write(dir.join("settings.json"), "{}").unwrap();
    let (_, id) = store.new_take_dir("20261015-120000-Adele-Hello").unwrap();
    for bad in ["..", ".", "", "../takes", "a/../..", "/etc", ".hidden", "..x", "x/y", "x\\y", "20261015..", "~"] {
        assert!(store.take_dir(bad).is_none(), "take_dir({bad:?}) must be refused");
        assert!(store.delete_take(bad).is_err(), "delete_take({bad:?}) must do nothing");
        assert!(!valid_take_id(bad));
    }
    assert!(dir.join("settings.json").exists() && dir.join("takes").is_dir(), "the data folder is untouched");
    assert_eq!(store.take_dir(&id), Some(dir.join("takes").join(&id)));
    assert!(valid_take_id("20261015-120000-Adele-Hello-2"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Finding 8: Take::try_align's first attempt took lock_seconds()*0.98 = 2.94 s of music, which onset_envelope
/// made 295 frames of; locate refuses anything under 300 and gave (0, 0.0), so the first placement after every
/// start, skip and resume was wasted and the next came 1 s later. Now the window the first try waits for gives
/// locate the frames it needs, and it places.
#[test]
fn first_placement_window_is_enough_for_locate() {
    use onkey_lib::dsp::{locate, onset_envelope, FP_BANDS, LOCATE_MIN_FRAMES};
    use onkey_lib::take::{align_samples, lock_seconds};
    let samples = align_samples(lock_seconds());
    let music = |i: usize| ((i as f32) * 0.05).sin() * 0.3 * (((i / 800) % 2) as f32);
    let x: Vec<f32> = (0..samples).map(music).collect();
    let env = onset_envelope(&x);
    println!("first try waits for {samples} samples ({:.3} s): {} frames", samples as f64 / 16_000.0, env.len());
    assert!(env.len() >= LOCATE_MIN_FRAMES, "the first window must give locate its {LOCATE_MIN_FRAMES} frames");
    // the window itself, inside a longer song, is found where it is
    let song: Vec<[f32; FP_BANDS]> = onset_envelope(&(0..samples * 4).map(music).collect::<Vec<f32>>());
    let (k, conf) = locate(&env, &song, 0, None);
    println!("locate -> ({k}, {conf})");
    assert!(conf > 0.0, "locate works on it (was (0, 0.0))");
    // and a lock time set below locate's minimum still waits for enough
    assert!(onset_envelope(&vec![0.1; align_samples(2.0)]).len() >= LOCATE_MIN_FRAMES);
}

/// Finding 1: the mic was the input device whose name contained "MacBook", so iMac, Mac mini and Mac Studio
/// users had none (silent takes) while setup's meter, on the default input, said "Heard you". Now both use
/// `chosen_mic`: a connected input device whenever there is one, and a preference that isn't connected falls
/// back to it. (Reads this Mac's devices; asks for no permission.)
#[test]
fn the_mic_is_chosen_from_the_input_devices_not_by_name() {
    use onkey_lib::capture::{chosen_mic, mics, set_mic_preference};
    let all = mics();
    let chosen = chosen_mic();
    println!("input devices: {:?}\nchosen: {chosen:?}", all.iter().map(|m| &m.name).collect::<Vec<_>>());
    assert_eq!(chosen.is_some(), !all.is_empty(), "a mic whenever there's an input device");
    if let Some(c) = &chosen {
        assert!(all.iter().any(|m| m.uid == c.uid), "the chosen mic is one of the input devices");
        assert!(!c.name.is_empty() && c.name != "OnKey");
    }
    set_mic_preference("no-such-device-uid");
    assert_eq!(chosen_mic(), chosen, "a chosen mic that isn't connected falls back to automatic");
    if let Some(last) = all.last() {
        set_mic_preference(&last.uid);
        assert_eq!(chosen_mic().map(|m| m.uid), Some(last.uid.clone()), "a connected chosen mic is used");
    }
    set_mic_preference("");
    assert_eq!(chosen_mic(), chosen);
}

/// Finding 3: two takes of one song learned at once (two learners finishing together) both call save_song; each
/// loaded the old song, merged its part and wrote, so the second write dropped the first's learning (40 of 40
/// rounds on the review's probe), and both shared one temp file <id>.bin.tmp. Now save_song holds the store's
/// write lock from load to write, and each write has its own temp file: both halves are kept, every round.
#[test]
fn concurrent_save_song_keeps_both_takes() {
    use onkey_lib::dsp::{FP_BANDS, FRAME_S};
    use onkey_lib::source::Track;
    use onkey_lib::store::Store;
    let dir = tmp("race");
    let store = Store::new(dir.clone());
    let n = (100.0 / FRAME_S) as usize;
    let onset: Vec<[f32; FP_BANDS]> = (0..n).map(|i| [((i * 37 % 101) as f32) / 10.0; FP_BANDS]).collect();
    let part = |a: usize, b: usize| (0..n).map(|i| (a..b).contains(&i)).collect::<Vec<bool>>();
    let t = Track { id: "isrc:RACE".into(), name: "Race".into(), artist: "A".into(), duration: 100.0, ..Default::default() };
    let (mut lost, mut corrupt, mut ok) = (0, 0, 0);
    for round in 0..40 {
        for ext in ["bin", "json"] { let _ = std::fs::remove_file(dir.join("songs").join(format!("isrc_RACE.{ext}"))); }
        let (s1, s2, t1, t2, o1, o2) = (store.clone(), store.clone(), t.clone(), t.clone(), onset.clone(), onset.clone());
        let (p1, p2) = (part(0, 5000), part(5000, 10000));
        let a = std::thread::spawn(move || s1.save_song(&t1, vec![60.0; n], p1, o1));
        let b = std::thread::spawn(move || s2.save_song(&t2, vec![62.0; n], p2, o2));
        let (ra, rb) = (a.join().unwrap(), b.join().unwrap());
        match store.load_song(&t.id) {
            None => { corrupt += 1; println!("round {round}: .bin unreadable after both saves (returned {ra}, {rb})"); }
            Some(l) => {
                let f = l.fraction(100.0);
                if f < 0.99 { lost += 1; println!("round {round}: only {:.0}% of the song kept (returned {ra}, {rb})", 100.0 * f); } else { ok += 1; }
            }
        }
    }
    println!("40 rounds: {ok} fine, {lost} lost a take's learning, {corrupt} corrupt .bin");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((lost, corrupt), (0, 0), "both takes' learning must be kept");
}

/// QA F13: a capture restart under a running take must not reset or misalign it. The time without capture goes
/// into the take as silence, so the take's clock (and the song time it shows) keeps up with the song, the echo
/// canceller and everything heard before are kept, and nothing before the gap moves.
#[test]
fn a_capture_restart_keeps_the_take_in_step_with_the_song() {
    use onkey_lib::source::Track;
    use onkey_lib::take::Take;
    let rate = 48_000.0;
    let track = Track { id: "isrc:GAP".into(), name: "Gap".into(), artist: "A".into(), duration: 200.0, ..Default::default() };
    let mut take = Take::new(track, None, false, rate, 3);
    let block: Vec<f32> = (0..4800).flat_map(|i| { let v = ((i as f32) * 0.03).sin() * 0.2; [v, v, 0.01 * v] }).collect();
    for _ in 0..50 { take.push(&block); }               // 5 s of music
    take.place(30.0 + 5.0);                             // the song is at 35 s
    let before = (take.song_time().unwrap(), take.mic16.len(), take.ref16.len());
    take.gap(1.25);                                     // the capture was down 1.25 s while the song played on
    for _ in 0..10 { take.push(&block); }               // 1 s more after the restart
    let t = take.song_time().unwrap();
    assert!((t - (before.0 + 1.25 + 1.0)).abs() < 1e-6, "song time {t} should be {} (no drift from the restart)", before.0 + 2.25);
    assert!((take.seconds() - 7.25).abs() < 1e-6);
    // what was heard before the gap is untouched, and the gap is silence of the right length at 16 kHz
    assert!(take.mic16.len() >= before.1 + 16_000 * 2 && take.ref16.len() >= before.2 + 16_000 * 2);
    assert!(take.ref16[before.2 + 1600..before.2 + 16_000].iter().all(|v| v.abs() < 1e-3), "the gap is silent");
    take.gap(0.0);                                      // nothing lost: nothing added
    assert!((take.seconds() - 7.25).abs() < 1e-6);
}

/// QA F13: with the music captured near-silent (an app's own volume turned right down), a tonal sound in the room
/// was scored as singing. While the music is too quiet the live line shows nothing, and the time is counted
/// (most of a take, and it's unsung: engine.rs score_take).
#[test]
fn no_live_line_while_the_music_is_too_quiet() {
    use onkey_lib::source::Track;
    use onkey_lib::take::Take;
    let rate = 48_000.0;
    let mut crepe = onkey_lib::pitch::Crepe::new().unwrap();
    let run = |quiet: bool, crepe: &mut onkey_lib::pitch::Crepe| {
        let track = Track { id: "isrc:QUIET".into(), name: "Quiet".into(), artist: "A".into(), duration: 200.0, ..Default::default() };
        let mut take = Take::new(track, None, false, rate, 22);
        take.music_quiet = quiet;
        let mut t = 0usize;
        for _ in 0..30 {
            // a clear 220 Hz voice in the mic, faint music
            let block: Vec<f32> = (0..4800).flat_map(|_| { t += 1; let x = t as f32 / 48_000.0;
                let v = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * x).sin(); [1e-4 * v, 1e-4 * v, v] }).collect();
            take.push(&block);
            take.step(crepe);
        }
        (take.usr.iter().filter(|v| !v.is_nan()).count(), take.quiet_s)
    };
    let (heard, q0) = run(false, &mut crepe);
    let (hidden, q1) = run(true, &mut crepe);
    println!("live pitch frames: {heard} with the music audible, {hidden} with it too quiet; quiet seconds {q0} / {q1}");
    assert!(heard > 100, "the voice is tracked normally");
    assert_eq!(hidden, 0, "no live line while the music is too quiet");
    assert!(q0 == 0.0 && (q1 - 3.0).abs() < 1e-6);
}
