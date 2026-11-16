use super::*;
use crate::coordinates::PageTransform;

fn target(id: usize, x: f64, y: f64) -> Target<usize> {
    Target::new(
        id,
        0,
        [
            Point { x, y },
            Point { x: x + 30., y },
            Point { x, y: y + 12. },
            Point {
                x: x + 30.,
                y: y + 12.,
            },
        ],
    )
}

fn pages() -> Vec<Page> {
    vec![Page::new(
        0,
        PageViewport::new(
            PageTransform::from_coefficients([1., 0., 0., 1., 0., 0.]),
            1.,
            Point { x: 0., y: 0. },
            Point { x: 0., y: 0. },
        )
        .unwrap(),
        Rect::new(0., 0., 1000., 1000.),
        Rect::new(0., 0., 1000., 1000.),
    )]
}

#[test]
fn snapshot_filters_visibility_and_subset_without_relabelling_on_prefix() {
    let snapshot = Snapshot::prepare(
        [
            target(7, 10., 30.),
            target(9, 80., 30.),
            target(10, 2000., 30.),
        ],
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(snapshot.labels().len(), 2);
    assert_eq!(*snapshot.labels()[0].target().id(), 7);
    assert_eq!(snapshot.labels()[1].letters(), "sa");
    let subset = snapshot.subset(|id| *id == 9);
    assert_eq!(subset.labels()[0].letters(), "sa");
    assert_eq!(*subset.labels()[0].target().id(), 9);
    let mut input = Input::default();
    assert_eq!(input.key("s", Some(&snapshot)), Outcome::Pending);
    assert!(input.matches(&snapshot.labels()[1]));
    assert_eq!(input.key("backspace", Some(&snapshot)), Outcome::Pending);
    assert_eq!(input.prefix(), "");
    input.key("z", Some(&snapshot));
    assert_eq!(input.key("z", Some(&snapshot)), Outcome::Pending);
    assert!(input.is_retry());
    assert_eq!(input.prefix(), "");
    input.key("s", Some(&snapshot));
    assert_eq!(input.key("a", Some(&snapshot)), Outcome::Chosen(1));
    assert_eq!(snapshot.labels()[1].letters(), "sa");
    assert_eq!(input.key("escape", Some(&snapshot)), Outcome::Cancelled);
}

#[test]
fn overlapping_targets_are_retained_but_capacity_still_refuses_entire_snapshot() {
    let overlapping = Snapshot::prepare(
        [target(1, 10., 30.), target(2, 11., 30.)],
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(overlapping.labels().len(), 2);
    assert_eq!(*overlapping.labels()[0].target().id(), 1);
    assert_eq!(*overlapping.labels()[1].target().id(), 2);
    let targets = |count| {
        (0..count).map(|ix| target(ix, (ix % 26) as f64 * 35., (ix / 26) as f64 * 30. + 20.))
    };
    let full = Snapshot::prepare(targets(676), &pages(), &[], Metrics::new(20., 12., 1.)).unwrap();
    assert_eq!(full.labels()[0].letters(), "aa");
    assert_eq!(full.labels()[1].letters(), "sa");
    assert_eq!(full.labels()[2].letters(), "da");
    assert_eq!(full.labels()[3].letters(), "fa");
    assert_eq!(full.labels()[25].letters(), "ma");
    assert_eq!(full.labels()[26].letters(), "as");
    assert_eq!(full.labels()[27].letters(), "ss");
    assert_eq!(full.labels()[28].letters(), "ds");
    assert_eq!(full.labels().last().unwrap().letters(), "mm");
    let mut input = Input::default();
    input.key("s", Some(&full));
    let matching: Vec<_> = full
        .labels()
        .iter()
        .enumerate()
        .filter(|(_, label)| input.matches(label))
        .map(|(ix, _)| ix)
        .collect();
    assert_eq!(matching, (0..26).map(|ix| 1 + ix * 26).collect::<Vec<_>>());
    let unique: std::collections::HashSet<_> =
        full.labels().iter().map(|label| label.letters()).collect();
    assert_eq!(unique.len(), 676);
    assert!(
        unique.iter().all(|letters| letters.len() == 2
            && letters.bytes().all(|letter| letter.is_ascii_lowercase()))
    );
    assert!(matches!(
        Snapshot::prepare(targets(677), &pages(), &[], Metrics::new(20., 12., 1.)),
        Err(Refusal::Crowded)
    ));
}

#[test]
fn paint_order_puts_leftmost_behind_without_changing_assignments() {
    let snapshot = Snapshot::prepare(
        [
            target(1, 200., 30.),
            target(2, 10., 30.),
            target(3, 100., 30.),
        ],
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    let input = Input::default();
    let painted: Vec<_> = snapshot
        .paint_labels(0, &input, None)
        .into_iter()
        .map(|label| (*label.target().id(), label.letters()))
        .collect();
    assert_eq!(painted, [(2, "sa"), (3, "da"), (1, "aa")]);
    assert_eq!(*snapshot.labels()[0].target().id(), 1);
    assert_eq!(snapshot.labels()[0].letters(), "aa");
    let mut input = Input::default();
    input.key("s", Some(&snapshot));
    assert_eq!(snapshot.paint_labels(0, &input, None)[0].letters(), "sa");
    assert!(snapshot.paint_labels(1, &input, None).is_empty());
    assert_eq!(snapshot.paint_labels(0, &input, Some(&1)).len(), 2);
}

#[test]
fn clips_partial_words_and_excludes_occlusion_and_missing_pages() {
    let snapshot = Snapshot::prepare(
        [
            target(1, -5., 5.),
            target(2, 80., 30.),
            Target::new(3, 1, [Point { x: 10., y: 30. }; 4]),
        ],
        &pages(),
        &[Rect::new(70., 0., 150., 60.)],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(snapshot.labels().len(), 1);
    assert_eq!(snapshot.labels()[0].bounds().edges(), [0., 5., 20., 17.]);
    assert!(matches!(
        Snapshot::prepare(
            [target(1, 999., 999.)],
            &pages(),
            &[],
            Metrics::new(20., 12., 1.)
        ),
        Err(Refusal::NoText)
    ));
}

#[test]
fn callers_choose_first_eligible_anchor_without_semantics_in_hinting() {
    let metrics = Metrics::new(20., 12., 1.);
    let pages = pages();
    let occlusions = [Rect::new(70., 0., 150., 60.)];
    let alternatives = [
        target(1, -40., 5.),
        target(1, -20., 5.),
        target(1, 80., 30.),
        target(1, -5., 5.),
        target(1, 200., 30.),
    ];
    let first = alternatives
        .into_iter()
        .find(|target| target.is_eligible(&pages, &occlusions, metrics))
        .unwrap();
    let snapshot = Snapshot::prepare([first], &pages, &occlusions, metrics).unwrap();
    assert_eq!(snapshot.labels().len(), 1);
    assert_eq!(*snapshot.labels()[0].target().id(), 1);
    assert_eq!(snapshot.labels()[0].bounds().edges(), [0., 5., 20., 17.]);
}

#[test]
fn short_word_can_use_space_after_it_instead_of_colliding_before_it() {
    let short = Target::new(
        2,
        0,
        [
            Point { x: 35., y: 30. },
            Point { x: 40., y: 30. },
            Point { x: 35., y: 42. },
            Point { x: 40., y: 42. },
        ],
    );
    let snapshot = Snapshot::prepare(
        [target(1, 10., 30.), short],
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(snapshot.labels()[1].bounds().edges(), [35., 18., 55., 30.]);
}

#[test]
fn clipped_slivers_are_ineligible_but_eligible_partial_collisions_retain_targets() {
    let snapshot = Snapshot::prepare(
        std::iter::repeat_with(|| target(0, 999., 999.))
            .take(677)
            .chain([target(1, -10., 5.), target(2, 80., 30.)]),
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(snapshot.labels().len(), 2);
    assert_eq!(*snapshot.labels()[0].target().id(), 1);
    assert_eq!(snapshot.labels()[0].bounds().edges(), [0., 5., 20., 17.]);
    assert_eq!(snapshot.labels()[0].letters(), "aa");
    let overlapping = Snapshot::prepare(
        [target(1, -10., 5.), target(2, -10., 5.)],
        &pages(),
        &[],
        Metrics::new(20., 12., 1.),
    )
    .unwrap();
    assert_eq!(overlapping.labels().len(), 2);
    assert!(matches!(
        Snapshot::prepare(
            [target(1, -10.01, 5.)],
            &pages(),
            &[],
            Metrics::new(20., 12., 1.),
        ),
        Err(Refusal::NoText)
    ));
}
