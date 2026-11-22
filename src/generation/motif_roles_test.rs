use super::*;

#[test]
fn assign_roles_empty_and_single() {
    let empty = assign_roles(&[]);
    assert_eq!(
        empty,
        RoleAssignment {
            primary: 0,
            secondary: 0,
            tertiary: 0,
        }
    );

    let single = assign_roles(&[MotifEnergy::new(42, 0.5)]);
    assert_eq!(
        single,
        RoleAssignment {
            primary: 42,
            secondary: 42,
            tertiary: 42,
        }
    );
}

#[test]
fn assign_roles_two_motifs() {
    let motifs = [MotifEnergy::new(1, 0.8), MotifEnergy::new(2, 0.2)];
    let roles = assign_roles(&motifs);
    assert_eq!(
        roles,
        RoleAssignment {
            primary: 2,
            secondary: 1,
            tertiary: 2,
        }
    );
}

#[test]
fn assign_roles_three_or_more() {
    let motifs = [
        MotifEnergy::new(10, 0.7),
        MotifEnergy::new(20, 0.1),
        MotifEnergy::new(30, 0.4),
    ];
    let roles = assign_roles(&motifs);
    assert_eq!(
        roles,
        RoleAssignment {
            primary: 20,
            secondary: 10,
            tertiary: 30,
        }
    );
}

#[test]
fn candidate_motifs_for_role_primary_and_secondary() {
    let motifs = [
        MotifEnergy::new(1, 0.2),
        MotifEnergy::new(2, 0.5),
        MotifEnergy::new(3, 0.8),
    ];
    let primary = candidate_motifs_for_role(&motifs, MotifRole::Primary, 1);
    assert_eq!(primary.ids, vec![1, 2]);
    assert_eq!(primary.weights, vec![3, 1]);

    let secondary = candidate_motifs_for_role(&motifs, MotifRole::Secondary, 3);
    assert_eq!(secondary.ids, vec![2, 3]);
    assert_eq!(secondary.weights, vec![1, 3]);

    let tertiary = candidate_motifs_for_role(&motifs, MotifRole::Tertiary, 2);
    assert_eq!(tertiary.ids, vec![1, 2, 3]);
    assert_eq!(tertiary.weights, vec![1, 3, 1]);
}
