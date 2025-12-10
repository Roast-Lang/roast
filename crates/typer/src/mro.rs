//! Method Resolution Order (MRO) calculation using C3 linearization.
//!
//! This module implements Python's C3 linearization algorithm for determining
//! the method resolution order in multiple inheritance scenarios.

use crate::types::{Type, ClassType};

/// Calculate the Method Resolution Order (MRO) for a class using C3 linearization.
///
/// The C3 algorithm produces a linearization that:
/// 1. Preserves local precedence order (order of base classes)
/// 2. Preserves monotonicity (if A comes before B in parent's MRO, A comes before B in child's MRO)
/// 3. Respects extended precedence graph
///
/// Returns the MRO as a list of class names, starting with the class itself.
pub fn compute_mro(class_name: &str, bases: &[Type], class_mros: &std::collections::HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    if bases.is_empty() {
        // No bases - MRO is just the class itself
        return Ok(vec![class_name.to_string()]);
    }

    // Collect base class names and their MROs
    let mut merge_lists: Vec<Vec<String>> = Vec::new();
    let mut base_names: Vec<String> = Vec::new();

    for base in bases {
        if let Type::Class(class_type) = base {
            let base_name = &class_type.name;
            base_names.push(base_name.clone());

            // Get or compute the base's MRO
            let base_mro = class_mros.get(base_name)
                .cloned()
                .unwrap_or_else(|| vec![base_name.clone()]);
            merge_lists.push(base_mro);
        }
    }

    // Add the list of direct bases at the end
    merge_lists.push(base_names.clone());

    // Start with the class itself
    let mut result = vec![class_name.to_string()];

    // C3 merge algorithm
    loop {
        // Remove empty lists
        merge_lists.retain(|l| !l.is_empty());

        if merge_lists.is_empty() {
            break;
        }

        // Find a good head: a class that doesn't appear in the tail of any list
        let mut found = None;
        for list in &merge_lists {
            let head = &list[0];
            let in_any_tail = merge_lists.iter().any(|l| l.len() > 1 && l[1..].contains(head));
            if !in_any_tail {
                found = Some(head.clone());
                break;
            }
        }

        match found {
            Some(head) => {
                result.push(head.clone());
                // Remove head from all lists
                for list in &mut merge_lists {
                    if !list.is_empty() && list[0] == head {
                        list.remove(0);
                    }
                }
            }
            None => {
                // No valid head found - inconsistent hierarchy
                return Err(format!(
                    "Cannot create a consistent method resolution order (MRO) for class '{}'. \
                     Check for circular inheritance or incompatible base class ordering.",
                    class_name
                ));
            }
        }
    }

    Ok(result)
}

/// Get the next class in the MRO after the current class.
/// Used for super() resolution.
pub fn get_next_in_mro(mro: &[String], current_class: &str) -> Option<String> {
    for (i, class) in mro.iter().enumerate() {
        if class == current_class && i + 1 < mro.len() {
            return Some(mro[i + 1].clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_class_type(name: &str) -> Type {
        Type::Class(ClassType {
            name: name.to_string(),
            module: None,
            type_params: Vec::new(),
            bases: Vec::new(),
            members: Vec::new(),
            methods: Vec::new(),
        })
    }

    #[test]
    fn test_single_inheritance() {
        let mut mros = std::collections::HashMap::new();
        mros.insert("Parent".to_string(), vec!["Parent".to_string()]);

        let bases = vec![make_class_type("Parent")];
        let mro = compute_mro("Child", &bases, &mros).unwrap();

        assert_eq!(mro, vec!["Child", "Parent"]);
    }

    #[test]
    fn test_diamond_inheritance() {
        // Classic diamond:
        //      A
        //     / \
        //    B   C
        //     \ /
        //      D
        let mut mros = std::collections::HashMap::new();
        mros.insert("A".to_string(), vec!["A".to_string()]);
        mros.insert("B".to_string(), vec!["B".to_string(), "A".to_string()]);
        mros.insert("C".to_string(), vec!["C".to_string(), "A".to_string()]);

        let bases = vec![make_class_type("B"), make_class_type("C")];
        let mro = compute_mro("D", &bases, &mros).unwrap();

        // D -> B -> C -> A (C3 linearization)
        assert_eq!(mro, vec!["D", "B", "C", "A"]);
    }

    #[test]
    fn test_multiple_inheritance() {
        let mut mros = std::collections::HashMap::new();
        mros.insert("A".to_string(), vec!["A".to_string()]);
        mros.insert("B".to_string(), vec!["B".to_string()]);

        let bases = vec![make_class_type("A"), make_class_type("B")];
        let mro = compute_mro("C", &bases, &mros).unwrap();

        assert_eq!(mro, vec!["C", "A", "B"]);
    }
}
