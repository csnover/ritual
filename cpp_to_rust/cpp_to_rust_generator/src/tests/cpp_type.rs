use cpp_ffi_data::CppTypeConversionToFfi;
use cpp_type::{
    CppBuiltInNumericType, CppClassType, CppFunctionPointerType, CppSpecificNumericType,
    CppSpecificNumericTypeKind, CppType, CppTypeRole,
};

fn assert_type_to_ffi_unchanged(t: &CppType) {
    for role in &[CppTypeRole::NotReturnType, CppTypeRole::ReturnType] {
        let ffi1 = t.to_cpp_ffi_type(role.clone()).unwrap();
        assert_eq!(&ffi1.original_type, t);
        assert_eq!(&ffi1.ffi_type, t);
        assert_eq!(ffi1.conversion, CppTypeConversionToFfi::NoChange);
    }
}

#[test]
fn void() {
    let type1 = CppType::Void;
    assert_eq!(type1.is_void(), true);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "void");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn void_ptr() {
    let type1 = CppType::new_pointer(false, CppType::Void);
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "void*");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn int() {
    let type1 = CppType::BuiltInNumeric(CppBuiltInNumericType::Int);
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "int");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn bool_ptr() {
    let type1 = CppType::new_pointer(false, CppType::BuiltInNumeric(CppBuiltInNumericType::Bool));
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "bool*");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn char_ptr_ptr() {
    let type1 = CppType::new_pointer(
        false,
        CppType::new_pointer(false, CppType::BuiltInNumeric(CppBuiltInNumericType::Char)),
    );
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "char**");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn qint64() {
    let type1 = CppType::SpecificNumeric(CppSpecificNumericType {
        name: "qint64".to_string(),
        bits: 64,
        kind: CppSpecificNumericTypeKind::Integer { is_signed: true },
    });
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "qint64");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn quintptr() {
    let type1 = CppType::PointerSizedInteger {
        name: "quintptr".to_string(),
        is_signed: false,
    };
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "quintptr");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn enum1() {
    let type1 = CppType::Enum {
        name: "Qt::CaseSensitivity".to_string(),
    };
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "Qt::CaseSensitivity");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn class_value() {
    let type1 = CppType::Class(CppClassType {
        name: "QPoint".to_string(),
        template_arguments: None,
    });
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "QPoint");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());

    let ffi_return_type = type1.to_cpp_ffi_type(CppTypeRole::ReturnType).unwrap();
    assert_eq!(&ffi_return_type.original_type, &type1);
    assert_eq!(
        &ffi_return_type.ffi_type,
        &CppType::new_pointer(
            false,
            CppType::Class(CppClassType {
                name: "QPoint".to_string(),
                template_arguments: None,
            })
        ),
    );
    assert_eq!(
        &ffi_return_type.ffi_type.to_cpp_code(None).unwrap(),
        "QPoint*"
    );
    assert_eq!(
        ffi_return_type.conversion,
        CppTypeConversionToFfi::ValueToPointer
    );

    let ffi_arg = type1.to_cpp_ffi_type(CppTypeRole::NotReturnType).unwrap();
    assert_eq!(&ffi_arg.original_type, &type1);
    assert_eq!(
        &ffi_arg.ffi_type,
        &CppType::new_pointer(
            true,
            CppType::Class(CppClassType {
                name: "QPoint".to_string(),
                template_arguments: None,
            })
        )
    );
    assert_eq!(
        &ffi_arg.ffi_type.to_cpp_code(None).unwrap(),
        "const QPoint*"
    );
    assert_eq!(ffi_arg.conversion, CppTypeConversionToFfi::ValueToPointer);
    assert!(type1.needs_allocation_place_variants());
}

#[test]
fn class_const_ref() {
    let type1 = CppType::new_reference(
        true,
        CppType::Class(CppClassType {
            name: "QRectF".to_string(),
            template_arguments: None,
        }),
    );
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "const QRectF&");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());

    for role in &[CppTypeRole::NotReturnType, CppTypeRole::ReturnType] {
        let ffi1 = type1.to_cpp_ffi_type(role.clone()).unwrap();
        assert_eq!(&ffi1.original_type, &type1);
        assert_eq!(
            &ffi1.ffi_type,
            &CppType::new_pointer(
                true,
                CppType::Class(CppClassType {
                    name: "QRectF".to_string(),
                    template_arguments: None,
                })
            )
        );
        assert_eq!(&ffi1.ffi_type.to_cpp_code(None).unwrap(), "const QRectF*");
        assert_eq!(ffi1.conversion, CppTypeConversionToFfi::ReferenceToPointer);
    }
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn class_mut_ref() {
    let type1 = CppType::new_reference(
        false,
        CppType::Class(CppClassType {
            name: "QRectF".to_string(),
            template_arguments: None,
        }),
    );
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "QRectF&");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());

    for role in &[CppTypeRole::NotReturnType, CppTypeRole::ReturnType] {
        let ffi1 = type1.to_cpp_ffi_type(role.clone()).unwrap();
        assert_eq!(&ffi1.original_type, &type1);
        assert_eq!(
            &ffi1.ffi_type,
            &CppType::new_pointer(
                false,
                CppType::Class(CppClassType {
                    name: "QRectF".to_string(),
                    template_arguments: None,
                })
            )
        );
        assert_eq!(&ffi1.ffi_type.to_cpp_code(None).unwrap(), "QRectF*");
        assert_eq!(ffi1.conversion, CppTypeConversionToFfi::ReferenceToPointer);
    }
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn class_mut_ptr() {
    let type1 = CppType::new_pointer(
        false,
        CppType::Class(CppClassType {
            name: "QObject".to_string(),
            template_arguments: None,
        }),
    );
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "QObject*");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn class_with_template_args() {
    let args = Some(vec![CppType::Class(CppClassType {
        name: "QString".to_string(),
        template_arguments: None,
    })]);
    let type1 = CppType::Class(CppClassType {
        name: "QVector".to_string(),
        template_arguments: args.clone(),
    });
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(type1.to_cpp_code(None).unwrap(), "QVector< QString >");
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());

    let ffi_return_type = type1.to_cpp_ffi_type(CppTypeRole::ReturnType).unwrap();
    assert_eq!(&ffi_return_type.original_type, &type1);
    assert_eq!(
        &ffi_return_type.ffi_type,
        &CppType::new_pointer(
            false,
            CppType::Class(CppClassType {
                name: "QVector".to_string(),
                template_arguments: args.clone(),
            })
        ),
    );
    assert_eq!(
        &ffi_return_type.ffi_type.to_cpp_code(None).unwrap(),
        "QVector< QString >*"
    );
    assert_eq!(
        ffi_return_type.conversion,
        CppTypeConversionToFfi::ValueToPointer
    );

    let ffi_arg = type1.to_cpp_ffi_type(CppTypeRole::NotReturnType).unwrap();
    assert_eq!(&ffi_arg.original_type, &type1);
    assert_eq!(
        &ffi_arg.ffi_type,
        &CppType::new_pointer(
            true,
            CppType::Class(CppClassType {
                name: "QVector".to_string(),
                template_arguments: args.clone(),
            })
        )
    );
    assert_eq!(
        &ffi_arg.ffi_type.to_cpp_code(None).unwrap(),
        "const QVector< QString >*"
    );
    assert_eq!(ffi_arg.conversion, CppTypeConversionToFfi::ValueToPointer);
    assert!(type1.needs_allocation_place_variants());
}

#[test]
fn nested_template_cpp_code() {
    let type1 = CppType::Class(CppClassType {
        name: "QHash".to_string(),
        template_arguments: Some(vec![
            CppType::Class(CppClassType {
                name: "QString".to_string(),
                template_arguments: None,
            }),
            CppType::Class(CppClassType {
                name: "QList".to_string(),
                template_arguments: Some(vec![CppType::Class(CppClassType {
                    name: "QString".to_string(),
                    template_arguments: None,
                })]),
            }),
        ]),
    });
    let code = type1.to_cpp_code(None).unwrap();
    assert_eq!(&code, "QHash< QString, QList< QString > >");
    assert!(!code.contains(">>"));
    assert!(!code.contains("<<"));
}

#[test]
fn qflags() {
    let args = Some(vec![CppType::Class(CppClassType {
        name: "Qt::AlignmentFlag".to_string(),
        template_arguments: None,
    })]);
    let type1 = CppType::Class(CppClassType {
        name: "QFlags".to_string(),
        template_arguments: args.clone(),
    });
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), true);
    assert_eq!(type1.is_template_parameter(), false);
    assert_eq!(
        type1.to_cpp_code(None).unwrap(),
        "QFlags< Qt::AlignmentFlag >"
    );
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());

    for role in &[CppTypeRole::NotReturnType, CppTypeRole::ReturnType] {
        let ffi_type = type1.to_cpp_ffi_type(role.clone()).unwrap();
        assert_eq!(&ffi_type.original_type, &type1);
        assert_eq!(
            &ffi_type.ffi_type,
            &CppType::BuiltInNumeric(CppBuiltInNumericType::UInt),
        );
        assert_eq!(
            &ffi_type.ffi_type.to_cpp_code(None).unwrap(),
            "unsigned int"
        );
        assert_eq!(ffi_type.conversion, CppTypeConversionToFfi::QFlagsToUInt);
    }
    assert!(!type1.needs_allocation_place_variants());
}

fn create_template_parameter_type() -> CppType {
    CppType::new_pointer(
        false,
        CppType::TemplateParameter {
            nested_level: 0,
            index: 0,
            name: "T".into(),
        },
    )
}

#[test]
fn template_parameter() {
    let type1 = create_template_parameter_type();
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), true);
    assert!(type1.to_cpp_code(None).is_err());
    assert!(type1.to_cpp_code(Some(&String::new())).is_err());
    assert!(type1.to_cpp_ffi_type(CppTypeRole::NotReturnType).is_err());
    assert!(type1.to_cpp_ffi_type(CppTypeRole::ReturnType).is_err());
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn function1() {
    let type1 = CppType::FunctionPointer(CppFunctionPointerType {
        allows_variadic_arguments: false,
        return_type: Box::new(CppType::BuiltInNumeric(CppBuiltInNumericType::Int)),
        arguments: vec![
            CppType::BuiltInNumeric(CppBuiltInNumericType::Int),
            CppType::new_pointer(false, CppType::BuiltInNumeric(CppBuiltInNumericType::Bool)),
        ],
    });
    assert_eq!(type1.is_void(), false);
    assert_eq!(type1.is_class(), false);
    assert_eq!(type1.is_template_parameter(), false);
    let name = "my_name".to_string();
    assert!(type1.to_cpp_code(None).is_err());
    assert_type_to_ffi_unchanged(&type1);
    assert!(!type1.needs_allocation_place_variants());
}

#[test]
fn instantiate1() {
    let type1 = CppType::new_reference(
        true,
        CppType::TemplateParameter {
            nested_level: 0,
            index: 0,
            name: "T".into(),
        },
    );
    let type2 = CppType::new_pointer(false, CppType::BuiltInNumeric(CppBuiltInNumericType::Bool));
    let r = type1.instantiate(0, &[type2]).unwrap();
    assert_eq!(
        r,
        CppType::new_reference(
            true,
            CppType::new_pointer(false, CppType::BuiltInNumeric(CppBuiltInNumericType::Bool))
        )
    );
}
