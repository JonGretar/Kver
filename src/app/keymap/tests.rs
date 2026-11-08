use super::*;
use std::collections::HashSet;

#[test]
fn defaults_have_no_duplicate_key_and_context_pairs() {
    let mut seen = HashSet::new();
    for binding in default_bindings() {
        let key = format!("{:?} {:?}", binding.keystrokes(), binding.predicate());
        assert!(seen.insert(key), "duplicate binding: {binding:?}");
        assert!(
            binding.action().name().starts_with("kver::"),
            "action identity changed: {binding:?}"
        );
    }
}

#[test]
fn every_mark_letter_has_set_jump_and_delete_bindings() {
    let bindings = default_bindings();
    for letter in 'a'..='z' {
        let actions: [(Box<dyn gpui_kit::Action>, usize); 3] = [
            (Box::new(SetLocalMark(letter)), 2),
            (Box::new(JumpLocalMark(letter)), 2),
            (Box::new(DeleteLocalMark(letter)), 3),
        ];
        for (action, length) in actions {
            let matches: Vec<_> = bindings
                .iter()
                .filter(|binding| binding.action().partial_eq(action.as_ref()))
                .collect();
            assert_eq!(matches.len(), 1, "{} for {letter}", action.name());
            assert_eq!(matches[0].keystrokes().len(), length);
            assert!(matches[0].predicate().is_some());
        }
    }
}
