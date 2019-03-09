use crate::cpp_data::CppPath;
use crate::cpp_data::CppTypeDeclarationKind;
use crate::cpp_type::CppPointerLikeTypeKind;
use crate::cpp_type::CppType;
use crate::processor::ProcessorData;
use log::trace;
use ritual_common::errors::Result;
use std::collections::HashMap;

pub fn set_allocation_places(data: &mut ProcessorData<'_>) -> Result<()> {
    for type1 in data
        .current_database
        .cpp_items_mut()
        .iter_mut()
        .filter_map(|item| item.cpp_data.as_type_mut())
    {
        let path = &type1.path;
        if let CppTypeDeclarationKind::Class { is_movable, .. } = &mut type1.kind {
            *is_movable = data.config.movable_types().iter().any(|p| p == path);
        }
    }
    Ok(())
}

/// Detects the preferred type allocation place for each type based on
/// API of all known methods. Doesn't actually change the data,
/// only suggests stack allocated types for manual configuration.
pub fn suggest_allocation_places(data: &mut ProcessorData<'_>) -> Result<()> {
    #[derive(Default, Debug)]
    struct TypeStats {
        // has_derived_classes: bool,
        has_virtual_methods: bool,
        pointers_count: usize,
        not_pointers_count: usize,
    };
    fn check_type(
        cpp_type: &CppType,
        is_behind_pointer: bool,
        data: &mut HashMap<CppPath, TypeStats>,
    ) {
        match cpp_type {
            CppType::Class(path) => {
                if !data.contains_key(path) {
                    data.insert(path.clone(), TypeStats::default());
                }
                if is_behind_pointer {
                    data.get_mut(path).unwrap().pointers_count += 1;
                } else {
                    data.get_mut(path).unwrap().not_pointers_count += 1;
                }
                if let Some(args) = &path.last().template_arguments {
                    for arg in args {
                        check_type(arg, false, data);
                    }
                }
            }
            CppType::PointerLike { kind, target, .. } => {
                check_type(target, *kind == CppPointerLikeTypeKind::Pointer, data);
            }
            _ => {}
        }
    }

    let mut data_map = HashMap::new();
    for type1 in data
        .current_database
        .cpp_items()
        .iter()
        .filter_map(|i| i.cpp_data.as_type_ref())
    {
        if data
            .current_database
            .cpp_items()
            .iter()
            .filter_map(|i| i.cpp_data.as_function_ref())
            .any(|m| m.class_type().ok().as_ref() == Some(&type1.path) && m.is_virtual())
        {
            // TODO: de-duplicate code for searching items by class_type
            if !data_map.contains_key(&type1.path) {
                data_map.insert(type1.path.clone(), TypeStats::default());
            }
            data_map.get_mut(&type1.path).unwrap().has_virtual_methods = true;
        }
    }
    for method in data
        .current_database
        .cpp_items()
        .iter()
        .filter_map(|i| i.cpp_data.as_function_ref())
    {
        for type1 in method.all_involved_types() {
            check_type(&type1, false, &mut data_map);
        }
    }

    for (name, stats) in &data_map {
        trace!("type = {}; stats = {:?}", name.to_cpp_pseudo_code(), stats);
    }

    let mut movable_types = Vec::new();
    let mut immovable_types = Vec::new();

    for type1 in data
        .current_database
        .cpp_items()
        .iter()
        .filter_map(|i| i.cpp_data.as_type_ref())
    {
        if !type1.kind.is_class() {
            continue;
        }
        let name = &type1.path;
        // TODO: add `heap_allocated_types` to `Config` just for suppressing the output of this function
        if data.config.movable_types().iter().any(|n| n == name) {
            continue;
        }
        let suggest_movable_types = if let Some(stats) = data_map.get(name) {
            if stats.has_virtual_methods {
                false
            } else if stats.pointers_count == 0 {
                true
            } else {
                let min_safe_data_count = 5;
                let min_not_pointers_percent = 0.3;
                if stats.pointers_count + stats.not_pointers_count < min_safe_data_count {
                    trace!(
                        "type = {}; Can't determine type allocation place: not enough data",
                        name.to_cpp_pseudo_code()
                    );
                } else if stats.not_pointers_count as f32
                    / (stats.pointers_count + stats.not_pointers_count) as f32
                    > min_not_pointers_percent
                {
                    trace!(
                        "type = {}; Can't determine type allocation place: many non-pointers",
                        name.to_cpp_pseudo_code()
                    );
                }
                false
            }
        } else {
            trace!(
                "type = {}; Can't determine type allocation place: no stats",
                name.to_cpp_pseudo_code()
            );
            false
        };

        if suggest_movable_types {
            movable_types.push(name.clone());
        } else {
            immovable_types.push(name.clone());
        }
    }

    trace!("Presumably immovable types: {:?}", immovable_types);
    trace!("Presumably movable types: {:?}", movable_types);

    Ok(())
}