use super::*;
use gpui_kit::{KeyContext, TestAppContext};
use std::collections::HashSet;

#[test]
fn later_layers_replace_a_key_slot_and_unbind_masks_a_global_key() {
    let map = resolve_with(
        Platform::Mac,
        Preset::Standard,
        [
            Entry::bind("cmd-o", Context::Global, NewWindow),
            Entry::unbind("cmd-o", Context::Reader),
        ],
    )
    .unwrap();
    assert_eq!(
        map.action("cmd-o", Context::Global),
        Some("kver::NewWindow")
    );
    assert_eq!(map.action("cmd-o", Context::Reader), None);
}

#[gpui_kit::test]
fn installed_policy_masks_global_keys_only_in_the_unbound_context(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::unbind("cmd-o", Context::Reader)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let keymap = cx.key_bindings();
        let key = parse("cmd-o", Platform::Mac.os()).unwrap();
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        let settings = [KeyContext::parse(key_context::SETTINGS).unwrap()];
        let bindings = keymap.borrow();
        assert!(bindings.bindings_for_input(&key, &reader).0.is_empty());
        assert_eq!(
            bindings.bindings_for_input(&key, &settings).0[0]
                .action()
                .name(),
            "kver::OpenPdf"
        );
    });
}

#[gpui_kit::test]
fn contents_unbind_masks_the_parent_sidebar_key(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::unbind("escape", Context::Contents)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let key = parse("escape", Platform::Mac.os()).unwrap();
        let sidebar = [KeyContext::parse(key_context::SIDEBAR).unwrap()];
        let contents = [
            KeyContext::parse(key_context::SIDEBAR).unwrap(),
            KeyContext::parse(key_context::CONTENTS).unwrap(),
        ];
        let keymap = cx.key_bindings();
        let bindings = keymap.borrow();
        assert_eq!(
            bindings.bindings_for_input(&key, &sidebar).0[0]
                .action()
                .name(),
            "kver::ToggleSidebar"
        );
        assert!(bindings.bindings_for_input(&key, &contents).0.is_empty());
    });
}

#[gpui_kit::test]
fn installed_contextual_action_wins_over_global_action(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let map = resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::bind("cmd-o", Context::Reader, NextPage)],
        )
        .unwrap();
        cx.bind_keys(map.bindings(cx));
        let key = parse("cmd-o", Platform::Mac.os()).unwrap();
        let reader = [KeyContext::parse(key_context::READER).unwrap()];
        let keymap = cx.key_bindings();
        assert_eq!(
            keymap.borrow().bindings_for_input(&key, &reader).0[0]
                .action()
                .name(),
            "kver::NextPage"
        );
    });
}

#[test]
fn platform_filters_entries_before_duplicate_detection() {
    let overrides = || {
        [
            Entry::bind("ctrl-e", Context::Reader, NextPage).on(Platform::Mac),
            Entry::bind("ctrl-e", Context::Reader, PreviousPage).on(Platform::Linux),
        ]
    };
    assert_eq!(
        resolve_with(Platform::Mac, Preset::Standard, overrides())
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::NextPage"),
    );
    assert_eq!(
        resolve_with(Platform::Linux, Preset::Standard, overrides())
            .unwrap()
            .action("ctrl-e", Context::Reader),
        Some("kver::PreviousPage"),
    );
    for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
        for preset in [Preset::Standard, Preset::Vim] {
            let map = resolve(platform, preset).unwrap();
            assert_eq!(map.action("n", Context::Reader), Some("kver::NextPage"));
            assert_eq!(map.action("cmd-o", Context::Global), Some("kver::OpenPdf"));
        }
    }
}

#[test]
fn overlapping_prefixes_and_duplicate_slots_are_reported() {
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::bind("m", Context::Reader, NextPage)],
        )
        .err(),
        Some(Diagnostic::PrefixConflict {
            first: "m".into(),
            second: "m a".into(),
            context: Context::Reader,
        }),
    );
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [
                Entry::bind("ctrl-e", Context::Reader, NextPage),
                Entry::bind("ctrl-e", Context::Reader, ScrollDown),
            ],
        )
        .err(),
        Some(Diagnostic::DuplicateSlot {
            keys: "ctrl-e".into(),
            context: Context::Reader,
        }),
    );
    assert!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [Entry::bind("m", Context::Settings, NextPage)],
        )
        .is_ok()
    );
    assert_eq!(
        resolve_with(
            Platform::Mac,
            Preset::Standard,
            [
                Entry::bind("ctrl-e", Context::Sidebar, NextPage),
                Entry::bind("ctrl-e p", Context::Contents, PreviousPage),
            ],
        )
        .err(),
        Some(Diagnostic::PrefixConflict {
            first: "ctrl-e".into(),
            second: "ctrl-e p".into(),
            context: Context::Contents,
        }),
    );
}

#[test]
fn defaults_have_no_duplicate_key_and_context_pairs() {
    let mut seen = HashSet::new();
    for slot in resolve(Platform::Mac, Preset::Standard).unwrap().slots {
        assert!(
            seen.insert((slot.sequence, slot.entry.context)),
            "duplicate slot: {}",
            slot.entry.keys
        );
        assert!(
            slot.entry.action.unwrap().name().starts_with("kver::"),
            "action identity changed: {}",
            slot.entry.keys
        );
    }
}

#[test]
fn every_mark_letter_has_set_jump_and_delete_bindings() {
    let map = resolve(Platform::Mac, Preset::Standard).unwrap();
    for letter in 'a'..='z' {
        let actions: [(Box<dyn gpui_kit::Action>, usize); 3] = [
            (Box::new(SetLocalMark(letter)), 2),
            (Box::new(JumpLocalMark(letter)), 2),
            (Box::new(DeleteLocalMark(letter)), 3),
        ];
        for (action, length) in actions {
            let matches: Vec<_> = map
                .slots
                .iter()
                .filter(|slot| {
                    slot.entry
                        .action
                        .as_ref()
                        .is_some_and(|binding| binding.partial_eq(action.as_ref()))
                })
                .collect();
            assert_eq!(matches.len(), 1, "{} for {letter}", action.name());
            assert_eq!(matches[0].sequence.len(), length);
            assert_eq!(matches[0].entry.context, Context::Reader);
        }
    }
}
