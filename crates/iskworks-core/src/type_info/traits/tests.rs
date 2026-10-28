use rust_decimal::Decimal;

use super::*;

const CALDARI_FRIGATE: i64 = 3330;
const ASSAULT_FRIGATES: i64 = 12095;

fn line(source: TypeBonusSource, bonus: Option<i64>, text: &str) -> TypeBonusLine {
    TypeBonusLine {
        source,
        bonus: bonus.map(Decimal::from),
        unit_id: bonus.map(|_| 105),
        unit_display_name: bonus.map(|_| "%".to_string()),
        text: text.to_string(),
    }
}

fn plain(text: &str) -> Vec<RichSegment> {
    vec![RichSegment::Text {
        text: text.to_string(),
        bold: false,
    }]
}

#[test]
fn the_harpy_reads_like_its_traits_tab() {
    let lines = [
        line(
            TypeBonusSource::Skill(CALDARI_FRIGATE),
            Some(4),
            "bonus to all shield resistances",
        ),
        line(
            TypeBonusSource::Skill(CALDARI_FRIGATE),
            Some(10),
            "<a href=showinfo:3301>Small Hybrid Turret</a> optimal range",
        ),
        line(
            TypeBonusSource::Skill(ASSAULT_FRIGATES),
            Some(5),
            "Small Hybrid Turret damage",
        ),
        line(
            TypeBonusSource::Role,
            None,
            "Can fit <a href=showinfo:47254>Assault Damage Controls</a>",
        ),
        line(
            TypeBonusSource::Role,
            Some(50),
            "reduction in Microwarpdrive signature radius penalty",
        ),
        line(
            TypeBonusSource::Misc,
            Some(50),
            "reduction in CONCORD bounties",
        ),
    ];
    let names = BTreeMap::from([
        (CALDARI_FRIGATE, "Caldari Frigate".to_string()),
        (ASSAULT_FRIGATES, "Assault Frigates".to_string()),
    ]);

    let groups = trait_groups(&lines, &names);

    let headings: Vec<(&str, Option<i64>, usize)> = groups
        .iter()
        .map(|group| {
            (
                group.heading.as_str(),
                group.skill_type_id,
                group.bonuses.len(),
            )
        })
        .collect();
    assert_eq!(
        headings,
        [
            (
                "Caldari Frigate bonuses (per skill level)",
                Some(CALDARI_FRIGATE),
                2
            ),
            (
                "Assault Frigates bonuses (per skill level)",
                Some(ASSAULT_FRIGATES),
                1
            ),
            ("Role Bonus", None, 2),
            ("Misc Bonus", None, 1),
        ]
    );
    assert_eq!(
        groups[0].bonuses[0],
        TraitBonus {
            amount: Some("4%".to_string()),
            text: plain("bonus to all shield resistances"),
        }
    );
    assert_eq!(
        groups[0].bonuses[1].text,
        [
            RichSegment::Link {
                type_id: 3301,
                text: "Small Hybrid Turret".to_string(),
            },
            RichSegment::Text {
                text: " optimal range".to_string(),
                bold: false,
            },
        ]
    );
    assert_eq!(groups[2].bonuses[0].amount, None);
}

#[test]
fn a_skill_the_import_lacks_still_gets_a_heading() {
    let groups = trait_groups(
        &[line(TypeBonusSource::Skill(77_777), Some(5), "bonus")],
        &BTreeMap::new(),
    );
    assert_eq!(groups[0].heading, "Skill bonuses (per skill level)");
}

#[test]
fn no_bonuses_no_groups() {
    assert_eq!(trait_groups(&[], &BTreeMap::new()), []);
}
