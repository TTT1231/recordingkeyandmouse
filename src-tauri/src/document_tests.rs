use super::*;
use crate::geometry::MonitorInfo;

fn movement(x: i32, y: i32) -> EventKind {
    EventKind::MouseMove { x, y }
}
fn down(button: MouseButton, x: i32, y: i32) -> EventKind {
    EventKind::MouseDown { button, x, y }
}
fn up(button: MouseButton, x: i32, y: i32) -> EventKind {
    EventKind::MouseUp { button, x, y }
}
fn key(vk: u32, down: bool) -> EventKind {
    if down {
        EventKind::KeyDown {
            vk,
            scan: 0,
            extended: false,
        }
    } else {
        EventKind::KeyUp {
            vk,
            scan: 0,
            extended: false,
        }
    }
}
fn doc(items: Vec<(i64, EventKind)>, duration: i64) -> TaskDocument {
    let geometry = DesktopGeometry {
        monitors: vec![MonitorInfo {
            name: "test".into(),
            x: -1000,
            y: -100,
            width: 3000,
            height: 2000,
            dpi: 120,
        }],
    };
    let mut doc = TaskDocument::new("测试".into(), geometry.clone(), "00000409".into());
    let events = items
        .into_iter()
        .enumerate()
        .map(|(i, (time_us, kind))| RecordedEvent {
            seq: i as u64 + 1,
            time_us,
            kind,
            native_time_ms: i as u32,
        })
        .collect();
    doc.replace_recording(RawRecording {
        gestures: Default::default(),
        started_at_wall_ms: 0,
        duration_us: duration,
        events,
        geometry,
        layout_klid: "00000409".into(),
    });
    doc
}
fn same(doc: &TaskDocument) -> String {
    serde_json::to_string(doc).unwrap()
}
const L: MouseButton = MouseButton::Left;
#[test]
fn grouping_keeps_every_interleaved_event_and_original_order() {
    let d = doc(
        vec![
            (10, down(L, 20, 20)),
            (20, key(0xA2, true)),
            (25, movement(50, 40)),
            (30, key(0x41, true)),
            (
                35,
                EventKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta: 120,
                    x: 50,
                    y: 40,
                },
            ),
            (40, key(0x41, false)),
            (45, movement(60, 50)),
            (50, key(0xA2, false)),
            (60, up(L, 60, 50)),
        ],
        100,
    );
    let mut refs: Vec<_> = d
        .actions
        .iter()
        .flat_map(|a| a.source_seqs.clone())
        .collect();
    refs.sort();
    assert_eq!(refs, (1..=9).collect::<Vec<_>>());
    assert_eq!(
        serde_json::to_string(&d.compile()).unwrap(),
        serde_json::to_string(&d.raw.as_ref().unwrap().events).unwrap()
    );
    assert_eq!(d.actions.len(), 3);
    assert!(matches!(d.actions[0].kind, ActionKind::Drag { .. }));
    d.validate_execution().unwrap();
}
#[test]
fn isolated_moves_update_endpoints_and_long_pauses_split_groups() {
    let d = doc(
        vec![
            (100, movement(-50, 20)),
            (150, movement(10, 30)),
            (400_000, movement(55, 80)),
        ],
        1_000_000,
    );
    assert_eq!(d.actions.len(), 2);
    assert!(matches!(
        d.actions[0].kind,
        ActionKind::Move { x: 10, y: 30 }
    ));
    assert!(matches!(
        d.actions[1].kind,
        ActionKind::Move { x: 55, y: 80 }
    ));
    assert_eq!(d.duration_us, 1_000_000);
}
#[test]
fn stationary_long_hold_and_small_jitter_are_not_drags() {
    let d = doc(
        vec![
            (0, down(L, 20, 20)),
            (100, movement(22, 19)),
            (1_000_000, up(L, 21, 20)),
        ],
        1_500_000,
    );
    assert_eq!(d.actions.len(), 1);
    assert!(matches!(d.actions[0].kind, ActionKind::Click { .. }));
    assert_eq!(d.compile().len(), 3);
}
#[test]
fn double_click_preserves_both_pairs_and_intermediate_jitter() {
    let d = doc(
        vec![
            (0, down(L, 20, 20)),
            (10, movement(21, 20)),
            (20, up(L, 21, 20)),
            (50, movement(22, 21)),
            (200_000, down(L, 22, 21)),
            (220_000, up(L, 22, 21)),
        ],
        500_000,
    );
    assert!(matches!(
        d.actions[0].kind,
        ActionKind::Click { count: 2, .. }
    ));
    assert_eq!(d.compile().len(), 6);
    assert_eq!(
        d.actions[0]
            .events
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::MouseDown { .. } | EventKind::MouseUp { .. }
            ))
            .count(),
        4
    );
    d.validate_execution().unwrap();
}
#[test]
fn inserted_wait_moves_all_later_tracks_preserving_pauses_and_raw() {
    let mut d = doc(
        vec![
            (100, movement(1, 1)),
            (500_000, key(0x41, true)),
            (550_000, key(0x41, false)),
            (800_000, down(L, 10, 20)),
            (850_000, up(L, 10, 20)),
        ],
        1_000_000,
    );
    let raw = serde_json::to_string(&d.raw).unwrap();
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::InsertWait {
        after_action: Some(id),
        duration_us: 1_000_000,
    })
    .unwrap();
    assert_eq!(
        d.compile().iter().map(|e| e.time_us).collect::<Vec<_>>(),
        vec![100, 1_500_000, 1_550_000, 1_800_000, 1_850_000]
    );
    assert_eq!(d.duration_us, 2_000_000);
    assert_eq!(raw, serde_json::to_string(&d.raw).unwrap());
    let wait = d
        .actions
        .iter()
        .find(|a| matches!(a.kind, ActionKind::Wait))
        .unwrap()
        .id
        .clone();
    d.apply(EditRequest::UpdateWait {
        action_id: wait,
        duration_us: 500_000,
    })
    .unwrap();
    assert_eq!(d.duration_us, 1_500_000);
    assert_eq!(d.compile()[1].time_us, 1_000_000);
    d.validate_execution().unwrap();
}
#[test]
fn open_input_boundary_rejects_edits_without_partial_changes() {
    let mut d = doc(
        vec![
            (0, down(L, 20, 20)),
            (10, key(0x41, true)),
            (20, key(0x41, false)),
            (30, up(L, 20, 20)),
        ],
        100,
    );
    let id = d
        .actions
        .iter()
        .find(|a| matches!(a.kind, ActionKind::Keys { .. }))
        .unwrap()
        .id
        .clone();
    let before = same(&d);
    assert!(
        d.apply(EditRequest::InsertWait {
            after_action: Some(id.clone()),
            duration_us: 1
        })
        .is_err()
    );
    assert_eq!(before, same(&d));
    assert!(
        d.apply(EditRequest::InsertText {
            after_action: Some(id),
            text: "x".into()
        })
        .is_err()
    );
    assert_eq!(before, same(&d));
}
#[test]
fn move_duration_shifts_same_timestamp_following_click_and_other_tracks() {
    let mut d = doc(
        vec![
            (10, movement(10, 20)),
            (110, movement(20, 30)),
            (110, down(L, 20, 30)),
            (120, up(L, 20, 30)),
            (200, key(0x41, true)),
            (210, key(0x41, false)),
        ],
        300,
    );
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::UpdateMove {
        action_id: id,
        x: 40,
        y: 50,
        duration_us: 200,
        link_next_click: false,
    })
    .unwrap();
    let e = d.compile();
    assert_eq!(
        e.iter().map(|e| e.time_us).collect::<Vec<_>>(),
        vec![10, 210, 210, 220, 300, 310]
    );
    assert_eq!(e[1].kind.position(), Some((40, 50)));
    assert_eq!(e[2].kind.position(), Some((20, 30)));
    assert_eq!(d.duration_us, 400);
    d.validate_execution().unwrap();
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::UpdateMove {
        action_id: id,
        x: 45,
        y: 60,
        duration_us: 200,
        link_next_click: true,
    })
    .unwrap();
    assert_eq!(d.compile()[2].kind.position(), Some((45, 60)));
}
#[test]
fn single_sample_move_allows_zero_duration_and_link_requires_actual_click() {
    let mut d = doc(vec![(100, movement(10, 20))], 300);
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::UpdateMove {
        action_id: id.clone(),
        x: 20,
        y: 30,
        duration_us: 0,
        link_next_click: false,
    })
    .unwrap();
    assert_eq!(d.compile().len(), 1);
    assert_eq!(d.compile()[0].kind.position(), Some((20, 30)));
    let before = same(&d);
    assert!(
        d.apply(EditRequest::UpdateMove {
            action_id: id,
            x: 25,
            y: 35,
            duration_us: 0,
            link_next_click: true
        })
        .is_err()
    );
    assert_eq!(before, same(&d));
}
#[test]
fn generated_clicks_chords_and_utf16_keep_equal_timestamp_order() {
    let mut d = doc(
        vec![
            (0, down(L, 20, 20)),
            (0, up(L, 20, 20)),
            (0, key(0x41, true)),
            (0, key(0x41, false)),
        ],
        100,
    );
    let click = d.actions[0].id.clone();
    let keys = d.actions[1].id.clone();
    d.apply(EditRequest::UpdateClick {
        action_id: click,
        button: MouseButton::Right,
        count: 2,
        x: 25,
        y: 30,
    })
    .unwrap();
    d.apply(EditRequest::UpdateKeys {
        action_id: keys.clone(),
        chord: "Ctrl + S".into(),
    })
    .unwrap();
    let e = d.compile();
    assert!(e[..4].iter().all(|e| matches!(
        e.kind,
        EventKind::MouseDown { .. } | EventKind::MouseUp { .. }
    )));
    d.validate_execution().unwrap();
    d.apply(EditRequest::UpdateText {
        action_id: keys.clone(),
        text: "中文😀\n!".into(),
    })
    .unwrap();
    let units: Vec<_> = d
        .compile()
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Unicode { unit, up: false } => Some(unit),
            _ => None,
        })
        .collect();
    assert_eq!(String::from_utf16(&units).unwrap(), "中文😀\n!");
    d.validate_execution().unwrap();
    d.apply(EditRequest::UpdateKeys {
        action_id: keys,
        chord: "Right".into(),
    })
    .unwrap();
    assert!(d.compile().iter().any(|e| matches!(
        e.kind,
        EventKind::KeyDown {
            vk: 0x27,
            extended: true,
            ..
        }
    )));
    d.validate_execution().unwrap();
}
#[test]
fn wheel_edit_preserves_native_units_positions_and_readonly_kinds() {
    let mut d = doc(
        vec![
            (
                0,
                EventKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta: 15,
                    x: 1,
                    y: 2,
                },
            ),
            (
                100,
                EventKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta: 30,
                    x: 2,
                    y: 3,
                },
            ),
            (
                200,
                EventKind::Wheel {
                    axis: WheelAxis::Horizontal,
                    delta: 120,
                    x: 2,
                    y: 3,
                },
            ),
        ],
        300,
    );
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::UpdateWheel {
        action_id: id,
        delta: -240,
    })
    .unwrap();
    let e = d.compile();
    assert_eq!(e[0].kind.position(), Some((1, 2)));
    assert_eq!(e[1].kind.position(), Some((2, 3)));
    assert_eq!(
        e.iter()
            .filter_map(|e| match e.kind {
                EventKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta,
                    ..
                } => Some(delta),
                _ => None,
            })
            .sum::<i32>(),
        -240
    );
    let id = d.actions[1].id.clone();
    let before = same(&d);
    assert!(
        d.apply(EditRequest::UpdateWheel {
            action_id: id,
            delta: 120
        })
        .is_err()
    );
    assert_eq!(before, same(&d));
}
#[test]
fn execution_rejects_orphan_keys_duplicate_buttons_and_offscreen_points() {
    assert!(
        doc(vec![(0, key(0x41, false))], 100)
            .validate_execution()
            .is_err()
    );
    assert!(
        doc(
            vec![(0, down(L, 1, 1)), (1, down(L, 1, 1)), (2, up(L, 1, 1))],
            100
        )
        .validate_execution()
        .is_err()
    );
    assert!(
        doc(vec![(0, movement(5000, 0))], 100)
            .validate_execution()
            .is_err()
    );
    doc(
        vec![
            (0, key(0x41, true)),
            (1, key(0x41, true)),
            (2, key(0x41, false)),
        ],
        100,
    )
    .validate_execution()
    .unwrap();
}

#[test]
fn coincident_move_notifications_share_click_or_wheel_without_dropping_sources() {
    let d = doc(
        vec![
            (0, movement(20, 20)),
            (100, down(L, 20, 20)),
            (200, up(L, 20, 20)),
            (500_000, movement(20, 20)),
            (
                500_100,
                EventKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta: 120,
                    x: 20,
                    y: 20,
                },
            ),
        ],
        1_000_000,
    );
    assert_eq!(d.actions.len(), 2);
    assert!(matches!(d.actions[0].kind, ActionKind::Click { .. }));
    assert!(matches!(d.actions[1].kind, ActionKind::Wheel { .. }));
    assert_eq!(d.compile().len(), 5);
    assert_eq!(
        d.actions.iter().map(|a| a.source_seqs.len()).sum::<usize>(),
        5
    );
    d.validate_execution().unwrap();
}
#[test]
fn shifting_text_keeps_declared_span_and_original_tail_pause() {
    let mut d = doc(
        vec![
            (0, movement(10, 10)),
            (100, key(0x41, true)),
            (200, key(0x41, false)),
        ],
        500,
    );
    let id = d.actions[1].id.clone();
    d.apply(EditRequest::UpdateText {
        action_id: id.clone(),
        text: "文".into(),
    })
    .unwrap();
    d.apply(EditRequest::InsertWait {
        after_action: Some(d.actions[0].id.clone()),
        duration_us: 1000,
    })
    .unwrap();
    let a = d.actions.iter().find(|a| a.id == id).unwrap();
    assert_eq!((a.start_us, a.end_us), (1100, 1200));
    assert_eq!(d.duration_us, 1500);
}

#[test]
fn double_click_obeys_recorded_platform_settings() {
    let d = doc(
        vec![
            (0, down(L, 20, 20)),
            (10, up(L, 20, 20)),
            (300_000, down(L, 23, 20)),
            (300_010, up(L, 23, 20)),
        ],
        400_000,
    );
    let raw = &d.raw.as_ref().unwrap().events;
    let rows = group_events_with_settings(
        raw,
        &GestureSettings {
            double_click_us: 200_000,
            radius_x: 4,
            radius_y: 4,
        },
    );
    assert_eq!(rows.len(), 2);
    let rows = group_events_with_settings(
        raw,
        &GestureSettings {
            double_click_us: 500_000,
            radius_x: 2,
            radius_y: 2,
        },
    );
    assert_eq!(rows.len(), 2);
    let rows = group_events_with_settings(
        raw,
        &GestureSettings {
            double_click_us: 500_000,
            radius_x: 4,
            radius_y: 4,
        },
    );
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].kind, ActionKind::Click { count: 2, .. }));
}

#[test]
fn chord_names_round_trip_through_vk_name_and_numpad_plus_is_one_key() {
    let vks = [
        0x08, 0x0D, 0x13, 0x14, 0x1B, 0x20, 0x21, 0x28, 0x2C, 0x2D, 0x2E, 0x30, 0x41, 0x5B, 0x5D,
        0x60, 0x69, 0x6A, 0x6B, 0x6D, 0x6E, 0x6F, 0x70, 0x87, 0xA0, 0xA5, 0xBA, 0xC0, 0xDB, 0xDE,
        0x3A,
    ];
    for vk in vks {
        let name = vk_name(vk);
        assert_eq!(
            parse_chord(&name),
            Ok(vec![vk]),
            "vk_name({vk:#04X}) 生成的「{name}」应能解析回原键"
        );
    }
    let chord = format!("{} + {} + {}", vk_name(0xA2), vk_name(0x6B), vk_name(0x41));
    assert_eq!(parse_chord(&chord), Ok(vec![0xA2, 0x6B, 0x41]));
    assert!(parse_chord("小键盘+ + A").is_ok());
}

#[test]
fn unchanged_keys_edit_keeps_repeats_and_original_timing() {
    let mut d = doc(
        vec![
            (0, key(0x41, true)),
            (200_000, key(0x41, true)),
            (400_000, key(0x41, true)),
            (600_000, key(0x41, false)),
        ],
        700_000,
    );
    let ActionKind::Keys { chord } = &d.actions[0].kind else {
        panic!("应是键盘动作");
    };
    assert_eq!(chord, "A");
    let before = same(&d);
    let id = d.actions[0].id.clone();
    d.apply(EditRequest::UpdateKeys {
        action_id: id.clone(),
        chord: "A".into(),
    })
    .unwrap();
    assert_eq!(before, same(&d), "原样确认不能重建事件或丢掉重复按下");
    d.apply(EditRequest::UpdateKeys {
        action_id: id,
        chord: "Ctrl + 小键盘+".into(),
    })
    .unwrap();
    let ActionKind::Keys { chord } = &d.actions[0].kind else {
        panic!("编辑后仍是键盘动作");
    };
    assert_eq!(chord, "左Ctrl + 小键盘+");
    d.validate_execution().unwrap();
}
