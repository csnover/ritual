//! Types and functions used for Rust code generation.

#![allow(dead_code)]

use crate::doc_formatter;
use crate::rust_info::RustDatabase;
use crate::rust_info::RustDatabaseItem;
use crate::rust_info::RustEnumValue;
use crate::rust_info::RustFFIFunction;
use crate::rust_info::RustFfiWrapperData;
use crate::rust_info::RustFunction;
use crate::rust_info::RustFunctionArgument;
use crate::rust_info::RustFunctionKind;
use crate::rust_info::RustFunctionScope;
use crate::rust_info::RustItemKind;
use crate::rust_info::RustModule;
use crate::rust_info::RustStruct;
use crate::rust_info::RustStructKind;
use crate::rust_info::RustWrapperTypeKind;
use crate::rust_type::CompleteType;
use crate::rust_type::RustPath;
use crate::rust_type::RustPointerLikeTypeKind;
use crate::rust_type::RustToFfiTypeConversion;
use crate::rust_type::RustType;
use itertools::Itertools;
use ritual_common::errors::{bail, err_msg, unexpected, Result};
use ritual_common::utils::MapIfOk;
use std::io::Write;

/// Generates Rust code representing type `rust_type` inside crate `crate_name`.
/// Same as `RustCodeGenerator::rust_type_to_code`, but accessible by other modules.
pub fn rust_type_to_code(rust_type: &RustType, current_crate: &str) -> String {
    match *rust_type {
        RustType::EmptyTuple => "()".to_string(),
        RustType::PointerLike {
            ref kind,
            ref target,
            ref is_const,
        } => {
            let target_code = rust_type_to_code(target, current_crate);
            match *kind {
                RustPointerLikeTypeKind::Pointer => {
                    if *is_const {
                        format!("*const {}", target_code)
                    } else {
                        format!("*mut {}", target_code)
                    }
                }
                RustPointerLikeTypeKind::Reference { ref lifetime } => {
                    let lifetime_text = match *lifetime {
                        Some(ref lifetime) => format!("'{} ", lifetime),
                        None => String::new(),
                    };
                    if *is_const {
                        format!("&{}{}", lifetime_text, target_code)
                    } else {
                        format!("&{}mut {}", lifetime_text, target_code)
                    }
                }
            }
        }
        RustType::Common {
            ref path,
            ref generic_arguments,
            ..
        } => {
            let mut code = path.full_name(Some(current_crate));
            if let Some(ref args) = *generic_arguments {
                code = format!(
                    "{}<{}>",
                    code,
                    args.iter()
                        .map(|x| rust_type_to_code(x, current_crate))
                        .join(", ",)
                );
            }
            code
        }
        RustType::FunctionPointer {
            ref return_type,
            ref arguments,
        } => format!(
            "extern \"C\" fn({}){}",
            arguments
                .iter()
                .map(|arg| rust_type_to_code(arg, current_crate))
                .join(", "),
            match return_type.as_ref() {
                &RustType::EmptyTuple => String::new(),
                return_type => format!(" -> {}", rust_type_to_code(return_type, current_crate)),
            }
        ),
    }
}

struct Generator<W> {
    #[allow(dead_code)]
    crate_name: String,
    destination: W,
}

/// Generates documentation comments containing
/// markdown code `doc`.
fn format_doc(doc: &str) -> String {
    fn format_line(x: &str) -> String {
        let mut line = format!("/// {}\n", x);
        if line.starts_with("///     ") {
            // block doc tests
            line = line.replace("///     ", "/// &#32;   ");
        }
        line
    }
    if doc.is_empty() {
        String::new()
    } else {
        doc.split('\n').map(format_line).join("")
    }
}

impl<W: Write> Generator<W> {
    pub fn generate_item(
        &mut self,
        item: &RustDatabaseItem,
        database: &RustDatabase,
    ) -> Result<()> {
        match item.kind {
            RustItemKind::Module(ref module) => self.generate_module(module, database),
            RustItemKind::Struct(ref data) => self.generate_struct(data, database),
            RustItemKind::EnumValue(ref value) => self.generate_enum_value(value),
            RustItemKind::TraitImpl(_) => unimplemented!(),
            RustItemKind::Function(_) => unimplemented!(),
        }
    }

    pub fn rust_type_to_code(&self, rust_type: &RustType) -> String {
        rust_type_to_code(rust_type, &self.crate_name)
    }

    pub fn generate_module(&mut self, module: &RustModule, database: &RustDatabase) -> Result<()> {
        writeln!(
            self.destination,
            "{}",
            format_doc(&doc_formatter::module_doc(&module.doc))
        )?;
        writeln!(self.destination, "pub mod {} {{", module.path.last())?;

        for item in database.children(&module.path) {
            self.generate_item(item, database)?;
        }

        writeln!(self.destination, "}}")?;
        Ok(())
    }

    pub fn generate_struct(
        &mut self,
        rust_struct: &RustStruct,
        database: &RustDatabase,
    ) -> Result<()> {
        writeln!(
            self.destination,
            "{}",
            format_doc(&doc_formatter::struct_doc(rust_struct))
        )?;
        let visibility = if rust_struct.is_public { "pub " } else { "" };
        match rust_struct.kind {
            RustStructKind::WrapperType(ref wrapper) => match wrapper.kind {
                RustWrapperTypeKind::EnumWrapper => {
                    writeln!(
                        self.destination,
                        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]"
                    )?;
                    writeln!(
                        self.destination,
                        "{}struct {}(::std::os::raw::c_int);",
                        visibility,
                        rust_struct.path.last()
                    )?;
                    writeln!(self.destination)?;
                }
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        }

        if database.children(&rust_struct.path).next().is_some() {
            writeln!(self.destination, "impl {} {{", rust_struct.path.last())?;
            for item in database.children(&rust_struct.path) {
                self.generate_item(item, database)?;
            }
            writeln!(self.destination, "}}")?;
            writeln!(self.destination)?;
        }

        Ok(())
    }

    pub fn generate_enum_value(&mut self, value: &RustEnumValue) -> Result<()> {
        let struct_path =
            self.rust_path_to_string(&value.path.parent().expect("enum value must have parent"));
        writeln!(
            self.destination,
            "pub const {value_name}: {struct_path} = {struct_path}({value});",
            value_name = value.path.last(),
            struct_path = struct_path,
            value = value.value
        )?;
        Ok(())
    }

    // TODO: generate relative paths for better readability
    pub fn rust_path_to_string(&self, path: &RustPath) -> String {
        path.full_name(Some(&self.crate_name))
    }

    /// Generates Rust code containing declaration of a FFI function `func`.
    fn rust_ffi_function_to_code(&self, func: &RustFFIFunction) -> String {
        let mut args = func.arguments.iter().map(|arg| {
            format!(
                "{}: {}",
                arg.name,
                self.rust_type_to_code(&arg.argument_type)
            )
        });
        format!(
            "  pub fn {}({}){};\n",
            func.path.last(),
            args.join(", "),
            match func.return_type {
                RustType::EmptyTuple => String::new(),
                _ => format!(" -> {}", self.rust_type_to_code(&func.return_type)),
            }
        )
    }

    /// Wraps `expression` of type `type1.rust_ffi_type` to convert
    /// it to type `type1.rust_api_type`.
    /// If `in_unsafe_context` is `true`, the output code will be placed inside
    /// an `unsafe` block.
    /// If `use_ffi_result_var` is `true`, the output code will assign
    /// the value to a temporary variable `ffi_result` and return it.
    fn convert_type_from_ffi(
        &self,
        type1: &CompleteType,
        expression: String,
        in_unsafe_context: bool,
        use_ffi_result_var: bool,
    ) -> Result<String> {
        let (unsafe_start, unsafe_end) = if in_unsafe_context {
            ("", "")
        } else {
            ("unsafe { ", " }")
        };
        if type1.rust_api_to_ffi_conversion == RustToFfiTypeConversion::None {
            return Ok(expression);
        }

        let (code1, source_expr) = if use_ffi_result_var {
            (
                format!("let ffi_result = {};\n", expression),
                "ffi_result".to_string(),
            )
        } else {
            (String::new(), expression)
        };
        let code2 = match type1.rust_api_to_ffi_conversion {
            RustToFfiTypeConversion::None => unreachable!(),
            RustToFfiTypeConversion::RefToPtr | RustToFfiTypeConversion::OptionRefToPtr => {
                let api_is_const = if type1.rust_api_to_ffi_conversion
                    == RustToFfiTypeConversion::OptionRefToPtr
                {
                    if let RustType::Common {
                        ref generic_arguments,
                        ..
                    } = type1.rust_api_type
                    {
                        let args = generic_arguments
                            .as_ref()
                            .ok_or_else(|| err_msg("Option with no generic_arguments"))?;
                        if args.len() != 1 {
                            bail!("Option with invalid args count");
                        }
                        args[0].last_is_const()?
                    } else {
                        bail!("Option type expected");
                    }
                } else {
                    type1.rust_api_type.last_is_const()?
                };
                let unwrap_code = match type1.rust_api_to_ffi_conversion {
                    RustToFfiTypeConversion::RefToPtr => {
                        ".expect(\"Attempted to convert null pointer to reference\")"
                    }
                    RustToFfiTypeConversion::OptionRefToPtr => "",
                    _ => unreachable!(),
                };
                format!(
                    "{unsafe_start}{}.{}(){unsafe_end}{}",
                    source_expr,
                    if api_is_const { "as_ref" } else { "as_mut" },
                    unwrap_code,
                    unsafe_start = unsafe_start,
                    unsafe_end = unsafe_end
                )
            }
            RustToFfiTypeConversion::ValueToPtr => format!(
                "{unsafe_start}*{}{unsafe_end}",
                source_expr,
                unsafe_start = unsafe_start,
                unsafe_end = unsafe_end
            ),
            RustToFfiTypeConversion::CppBoxToPtr => format!(
                "{unsafe_start}::cpp_utils::CppBox::new({}){unsafe_end}",
                source_expr,
                unsafe_start = unsafe_start,
                unsafe_end = unsafe_end
            ),
            RustToFfiTypeConversion::QFlagsToUInt => {
                let mut qflags_type = type1.rust_api_type.clone();
                if let RustType::Common {
                    ref mut generic_arguments,
                    ..
                } = qflags_type
                {
                    *generic_arguments = None;
                } else {
                    unreachable!();
                }
                format!(
                    "{}::from_int({} as i32)",
                    self.rust_type_to_code(&qflags_type),
                    source_expr
                )
            }
        };
        Ok(code1 + &code2)
    }

    /// Generates Rust code for calling an FFI function from a wrapper function.
    /// If `in_unsafe_context` is `true`, the output code will be placed inside
    /// an `unsafe` block.
    fn generate_ffi_call(
        &self,
        arguments: &[RustFunctionArgument],
        return_type: &CompleteType,
        wrapper_data: &RustFfiWrapperData,
        in_unsafe_context: bool,
    ) -> Result<String> {
        let (unsafe_start, unsafe_end) = if in_unsafe_context {
            ("", "")
        } else {
            ("unsafe { ", " }")
        };
        let mut final_args = Vec::new();
        final_args.resize(wrapper_data.cpp_ffi_function.arguments.len(), None);
        let all_args: Vec<RustFunctionArgument> = Vec::from(arguments);
        for arg in &all_args {
            assert!(arg.ffi_index < final_args.len());
            let mut code = arg.name.clone();
            match arg.argument_type.rust_api_to_ffi_conversion {
                RustToFfiTypeConversion::None => {}
                RustToFfiTypeConversion::OptionRefToPtr => {
                    bail!("OptionRefToPtr is not supported here yet");
                }
                RustToFfiTypeConversion::RefToPtr => {
                    if arg.argument_type.rust_api_type.is_const()?
                        && !arg.argument_type.rust_ffi_type.is_const()?
                    {
                        let mut intermediate_type = arg.argument_type.rust_ffi_type.clone();
                        intermediate_type.set_const(true)?;
                        code = format!(
                            "{} as {} as {}",
                            code,
                            self.rust_type_to_code(&intermediate_type),
                            self.rust_type_to_code(&arg.argument_type.rust_ffi_type)
                        );
                    } else {
                        code = format!(
                            "{} as {}",
                            code,
                            self.rust_type_to_code(&arg.argument_type.rust_ffi_type)
                        );
                    }
                }
                RustToFfiTypeConversion::ValueToPtr | RustToFfiTypeConversion::CppBoxToPtr => {
                    let is_const = if let RustType::PointerLike { ref is_const, .. } =
                        arg.argument_type.rust_ffi_type
                    {
                        *is_const
                    } else {
                        unexpected!("void is not expected here at all!");
                    };
                    if arg.argument_type.rust_api_to_ffi_conversion
                        == RustToFfiTypeConversion::CppBoxToPtr
                    {
                        let method = if is_const { "as_ptr" } else { "as_mut_ptr" };
                        code = format!("{}.{}()", code, method);
                    } else {
                        code = format!(
                            "{}{} as {}",
                            if is_const { "&" } else { "&mut " },
                            code,
                            self.rust_type_to_code(&arg.argument_type.rust_ffi_type)
                        );
                    }
                }
                RustToFfiTypeConversion::QFlagsToUInt => {
                    code = format!("{}.to_int() as ::libc::c_uint", code);
                }
            }
            final_args[arg.ffi_index] = Some(code);
        }

        let mut result = Vec::new();
        let mut maybe_result_var_name = None;
        if let Some(ref i) = wrapper_data.return_type_ffi_index {
            let mut return_var_name = "object".to_string();
            let mut ii = 1;
            while arguments.iter().any(|x| x.name == return_var_name) {
                ii += 1;
                return_var_name = format!("object{}", ii);
            }
            let struct_name =
                if return_type.rust_api_to_ffi_conversion == RustToFfiTypeConversion::CppBoxToPtr {
                    if let RustType::Common {
                        ref generic_arguments,
                        ..
                    } = return_type.rust_api_type
                    {
                        let generic_arguments = generic_arguments
                            .as_ref()
                            .ok_or_else(|| err_msg("CppBox must have generic_arguments"))?;
                        let arg = generic_arguments.get(0).ok_or_else(|| {
                            err_msg("CppBox must have non-empty generic_arguments")
                        })?;
                        self.rust_type_to_code(arg)
                    } else {
                        unexpected!("CppBox type expected");
                    }
                } else {
                    self.rust_type_to_code(&return_type.rust_api_type)
                };
            result.push(format!(
                "{{\nlet mut {var}: {t} = {unsafe_start}\
                 ::cpp_utils::new_uninitialized::NewUninitialized::new_uninitialized()\
                 {unsafe_end};\n",
                var = return_var_name,
                t = struct_name,
                unsafe_start = unsafe_start,
                unsafe_end = unsafe_end
            ));
            final_args[*i as usize] = Some(format!("&mut {}", return_var_name));
            maybe_result_var_name = Some(return_var_name);
        }
        let final_args = final_args
            .into_iter()
            .map_if_ok(|x| x.ok_or_else(|| err_msg("ffi argument is missing")))?;

        result.push(format!(
            "{unsafe_start}{}({}){maybe_semicolon}{unsafe_end}",
            self.rust_path_to_string(&wrapper_data.ffi_function_path),
            final_args.join(", "),
            maybe_semicolon = if maybe_result_var_name.is_some() {
                ";"
            } else {
                ""
            },
            unsafe_start = unsafe_start,
            unsafe_end = unsafe_end
        ));
        if let Some(ref name) = maybe_result_var_name {
            result.push(format!("{}\n}}", name));
        }
        let code = result.join("");
        if maybe_result_var_name.is_none() {
            self.convert_type_from_ffi(&return_type, code, in_unsafe_context, true)
        } else {
            Ok(code)
        }
    }

    /// Generates Rust code for declaring a function's arguments.
    fn arg_texts(&self, args: &[RustFunctionArgument], lifetime: Option<&String>) -> Vec<String> {
        args.iter()
            .map(|arg| {
                if &arg.name == "self" {
                    let self_type = match lifetime {
                        Some(lifetime) => arg
                            .argument_type
                            .rust_api_type
                            .with_lifetime(lifetime.clone()),
                        None => arg.argument_type.rust_api_type.clone(),
                    };
                    match self_type {
                        RustType::Common { .. } => "self".to_string(),
                        RustType::PointerLike {
                            ref kind,
                            ref is_const,
                            ..
                        } => {
                            if let RustPointerLikeTypeKind::Reference { ref lifetime } = *kind {
                                let maybe_mut = if *is_const { "" } else { "mut " };
                                match *lifetime {
                                    Some(ref lifetime) => {
                                        format!("&'{} {}self", lifetime, maybe_mut)
                                    }
                                    None => format!("&{}self", maybe_mut),
                                }
                            } else {
                                panic!("invalid self argument type (indirection)");
                            }
                        }
                        _ => {
                            panic!("invalid self argument type (not Common)");
                        }
                    }
                } else {
                    let mut maybe_mut_declaration = "";
                    if let RustType::Common { .. } = arg.argument_type.rust_api_type {
                        if arg.argument_type.rust_api_to_ffi_conversion
                            == RustToFfiTypeConversion::ValueToPtr
                        {
                            if let RustType::PointerLike { ref is_const, .. } =
                                arg.argument_type.rust_ffi_type
                            {
                                if !*is_const {
                                    maybe_mut_declaration = "mut ";
                                }
                            }
                        }
                    }

                    format!(
                        "{}{}: {}",
                        maybe_mut_declaration,
                        arg.name,
                        match lifetime {
                            Some(lifetime) => self.rust_type_to_code(
                                &arg.argument_type
                                    .rust_api_type
                                    .with_lifetime(lifetime.clone(),)
                            ),
                            None => self.rust_type_to_code(&arg.argument_type.rust_api_type),
                        }
                    )
                }
            })
            .collect()
    }

    /// Generates complete code of a Rust wrapper function.
    fn generate_rust_final_function(&self, func: &RustFunction) -> Result<String> {
        let maybe_pub = match func.scope {
            RustFunctionScope::TraitImpl => "",
            _ => "pub ",
        };
        let maybe_unsafe = if func.is_unsafe { "unsafe " } else { "" };

        let body = match func.kind {
            RustFunctionKind::FfiWrapper(ref data) => {
                self.generate_ffi_call(&func.arguments, &func.return_type, data, func.is_unsafe)?
            }
            RustFunctionKind::CppDeletableImpl { .. } => unimplemented!(),
            RustFunctionKind::SignalOrSlotGetter { .. } => unimplemented!(),
        };

        let return_type_for_signature = if func.return_type.rust_api_type == RustType::EmptyTuple {
            String::new()
        } else {
            format!(
                " -> {}",
                self.rust_type_to_code(&func.return_type.rust_api_type)
            )
        };
        let all_lifetimes: Vec<_> = func
            .arguments
            .iter()
            .filter_map(|x| x.argument_type.rust_api_type.lifetime())
            .collect();
        let lifetimes_text = if all_lifetimes.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                all_lifetimes.iter().map(|x| format!("'{}", x)).join(", ")
            )
        };

        Ok(format!(
            "{doc}{maybe_pub}{maybe_unsafe}fn {name}{lifetimes_text}({args}){return_type} \
             {{\n{body}}}\n\n",
            doc = format_doc(&doc_formatter::function_doc(&func)),
            maybe_pub = maybe_pub,
            maybe_unsafe = maybe_unsafe,
            lifetimes_text = lifetimes_text,
            name = func.path.last(),
            args = self.arg_texts(&func.arguments, None).join(", "),
            return_type = return_type_for_signature,
            body = body
        ))
    }
}

pub fn run(crate_name: &str, database: &RustDatabase, destination: impl Write) -> Result<()> {
    let mut generator = Generator {
        crate_name: crate_name.to_string(),
        destination,
    };

    let root = RustPath::from_parts(vec![crate_name.to_string()]);

    for item in database.children(&root) {
        generator.generate_item(item, database)?;
    }
    Ok(())
}

/*


impl<'a> RustCodeGenerator<'a> {







    /// Generates `lib.rs` file.
    #[allow(clippy::collapsible_if)]
    pub fn generate_lib_file(&self, modules: &[RustModule]) -> Result<()> {
        let mut code = String::new();

        code.push_str("pub extern crate libc;\n");
        code.push_str("pub extern crate cpp_utils;\n\n");
        for dep in self.config.generator_dependencies {
            code.push_str(&format!(
                "pub extern crate {};\n\n",
                &dep.rust_export_info.crate_name
            ));
        }

        // some ffi functions are not used because
        // some Rust methods are filtered
        code.push_str(
            "\
             #[allow(dead_code)]\nmod ffi { \ninclude!(concat!(env!(\"OUT_DIR\"), \
             \"/ffi.rs\")); \n}\n\n",
        );
        code.push_str(
            "\
             mod type_sizes { \ninclude!(concat!(env!(\"OUT_DIR\"), \
             \"/type_sizes.rs\")); \n}\n\n",
        );

        for name in &["ffi", "type_sizes"] {
            if modules.iter().any(|x| &x.name.as_str() == name) {
                return Err(format!(
                    "Automatically generated module '{}' conflicts with a mandatory \
                     module",
                    name
                )
                .into());
            }
        }
        for name in &["lib", "main"] {
            if modules.iter().any(|x| &x.name.as_str() == name) {
                return Err(format!(
                    "Automatically generated module '{}' conflicts with a reserved name",
                    name
                )
                .into());
            }
        }

        for module in modules {
            let doc = module
                .doc
                .as_ref()
                .map(|d| format_doc(d))
                .unwrap_or_default();
            code.push_str(&format!("{}pub mod {};\n", doc, &module.name));
        }

        let src_path = self.config.output_path.join("src");
        let lib_file_path = src_path.join("lib.rs");

        self.save_src_file(&lib_file_path, &code)?;
        self.call_rustfmt(&lib_file_path);
        Ok(())
    }

    /// Generates Rust code for given trait implementations.
    fn generate_trait_impls(&self, trait_impls: &[TraitImpl]) -> Result<String> {
        let mut results = Vec::new();
        for trait1 in trait_impls {
            let associated_types_text = trait1
                .associated_types
                .iter()
                .map(|t| format!("type {} = {};", t.name, self.rust_type_to_code(&t.value)))
                .join("\n");

            let trait_content =
                if let Some(TraitImplExtra::CppDeletable { ref deleter_name }) = trait1.extra {
                    format!(
                        "fn deleter() -> ::cpp_utils::Deleter<Self> {{\n  ::ffi::{}\n}}\n",
                        deleter_name
                    )
                } else {
                    trait1
                        .methods
                        .iter()
                        .map_if_ok(|method| self.generate_rust_final_function(method))?
                        .join("")
                };
            results.push(format!(
                "impl {} for {} {{\n{}{}}}\n\n",
                self.rust_type_to_code(&trait1.trait_type),
                self.rust_type_to_code(&trait1.target_type),
                associated_types_text,
                trait_content
            ));
        }
        Ok(results.join(""))
    }

    /// Generates code for a module of the output crate.
    /// This may be a top level or nested module.
    #[allow(clippy::single_match_else)]
    fn generate_module_code(&self, data: &RustModule) -> Result<String> {
        let mut results = Vec::new();
        for type1 in &data.types {
            results.push(format_doc(&doc_formatter::type_doc(type1)));
            let maybe_pub = if type1.is_public { "pub " } else { "" };
            match type1.kind {
                RustTypeDeclarationKind::CppTypeWrapper {
                    ref cpp_type_name,
                    ref kind,
                    ref methods,
                    ref trait_impls,
                    ref qt_receivers,
                    ..
                } => {
                    let r = match *kind {
                        RustTypeWrapperKind::Enum {
                            ref values,
                            ref is_flaggable,
                        } => {
                            let mut r = format!(
                                include_str!("../templates/crate/enum_declaration.rs.in"),
                                maybe_pub = maybe_pub,
                                name = type1.name.last_name()?,
                                variants = values
                                    .iter()
                                    .map(|item| format!(
                                        "{}  {} = {}",
                                        format_doc(&doc_formatter::enum_value_doc(&item)),
                                        item.name,
                                        item.value
                                    ))
                                    .join(", \n")
                            );
                            if *is_flaggable {
                                r = r + &format!(
                                    include_str!("../templates/crate/impl_flaggable.rs.in"),
                                    name = type1.name.last_name()?,
                                    trait_type = RustName::new(vec![
                                        "qt_core".to_string(),
                                        "flags".to_string(),
                                        "FlaggableEnum".to_string(),
                                    ])?
                                    .full_name(Some(&self.config.crate_properties.name()))
                                );
                            }
                            r
                        }
                        RustTypeWrapperKind::Struct {
                            ref size_const_name,
                            ref slot_wrapper,
                            ..
                        } => {
                            let mut r = if let Some(ref size_const_name) = *size_const_name {
                                format!(
                                    include_str!("../templates/crate/struct_declaration.rs.in"),
                                    maybe_pub = maybe_pub,
                                    name = type1.name.last_name()?,
                                    size_const_name = size_const_name
                                )
                            } else {
                                format!(
                                    "#[repr(C)]\n{maybe_pub}struct {}(u8);\n\n",
                                    type1.name.last_name()?,
                                    maybe_pub = maybe_pub
                                )
                            };

                            if let Some(ref slot_wrapper) = *slot_wrapper {
                                let arg_texts: Vec<_> = slot_wrapper
                                    .arguments
                                    .iter()
                                    .map(|t| self.rust_type_to_code(&t.rust_api_type))
                                    .collect();
                                let args = arg_texts.join(", ");
                                let args_tuple = format!(
                                    "{}{}",
                                    args,
                                    if arg_texts.len() == 1 { "," } else { "" }
                                );
                                let connections_mod = RustName::new(vec![
                                    "qt_core".to_string(),
                                    "connection".to_string(),
                                ])?
                                .full_name(Some(&self.config.crate_properties.name()));
                                let object_type_name = RustName::new(vec![
                                    "qt_core".to_string(),
                                    "object".to_string(),
                                    "Object".to_string(),
                                ])?
                                .full_name(Some(&self.config.crate_properties.name()));
                                r.push_str(&format!(
                                    include_str!(
                                        "../templates/crate/extern_slot_impl_receiver.rs.in"
                                    ),
                                    type_name = type1
                                        .name
                                        .full_name(Some(&self.config.crate_properties.name())),
                                    args_tuple = args_tuple,
                                    receiver_id = slot_wrapper.receiver_id,
                                    connections_mod = connections_mod,
                                    object_type_name = object_type_name
                                ));
                            }
                            r
                        }
                    };
                    results.push(r);
                    if !methods.is_empty() {
                        results.push(format!(
                            "impl {} {{\n{}}}\n\n",
                            type1.name.last_name()?,
                            methods
                                .iter()
                                .map_if_ok(|method| self.generate_rust_final_function(method))?
                                .join("")
                        ));
                    }
                    results.push(self.generate_trait_impls(trait_impls)?);
                    if !qt_receivers.is_empty() {
                        let connections_mod =
                            RustName::new(vec!["qt_core".to_string(), "connection".to_string()])?
                                .full_name(Some(&self.config.crate_properties.name()));
                        let object_type_name = RustName::new(vec![
                            "qt_core".to_string(),
                            "object".to_string(),
                            "Object".to_string(),
                        ])?
                        .full_name(Some(&self.config.crate_properties.name()));
                        let mut content = Vec::new();
                        let obj_name = type1
                            .name
                            .full_name(Some(&self.config.crate_properties.name()));
                        content.push("use ::cpp_utils::StaticCast;\n".to_string());
                        let mut type_impl_content = Vec::new();
                        for receiver_type in &[RustQtReceiverType::Signal, RustQtReceiverType::Slot]
                        {
                            if qt_receivers
                                .iter()
                                .any(|r| &r.receiver_type == receiver_type)
                            {
                                let (struct_method, struct_type, struct_method_doc) =
                                    match *receiver_type {
                                        RustQtReceiverType::Signal => (
                                            "signals",
                                            "Signals",
                                            "Provides access to built-in Qt signals of this type",
                                        ),
                                        RustQtReceiverType::Slot => (
                                            "slots",
                                            "Slots",
                                            "Provides access to built-in Qt slots of this type",
                                        ),
                                    };
                                let mut struct_content = Vec::new();
                                content.push(format!(
                                    "{}pub struct {}<'a>(&'a {});\n",
                                    format_doc(
                                        &doc_formatter::doc_for_qt_builtin_receivers_struct(
                                            type1.name.last_name()?,
                                            struct_method,
                                        ),
                                    ),
                                    struct_type,
                                    obj_name
                                ));
                                for receiver in qt_receivers {
                                    if &receiver.receiver_type == receiver_type {
                                        let arg_texts: Vec<_> = receiver
                                            .arguments
                                            .iter()
                                            .map(|t| self.rust_type_to_code(t))
                                            .collect();
                                        let args_tuple = arg_texts.join(", ")
                                            + if arg_texts.len() == 1 { "," } else { "" };
                                        content.push(format!(
                                            "{}pub struct {}<'a>(&'a {});\n",
                                            format_doc(
                                                &doc_formatter::doc_for_qt_builtin_receiver(
                                                    cpp_type_name,
                                                    type1.name.last_name()?,
                                                    receiver,
                                                )
                                            ),
                                            receiver.type_name,
                                            obj_name
                                        ));
                                        content.push(format!(
                                            "\
impl<'a> {connections_mod}::Receiver for {type_name}<'a> {{
  type Arguments = ({arguments});
  fn object(&self) -> &{object_type_name} {{ self.0.static_cast() }}
  fn receiver_id() -> &'static [u8] {{ b\"{receiver_id}\\0\" }}
}}\n",
                                            type_name = receiver.type_name,
                                            arguments = args_tuple,
                                            connections_mod = connections_mod,
                                            object_type_name = object_type_name,
                                            receiver_id = receiver.receiver_id
                                        ));
                                        if *receiver_type == RustQtReceiverType::Signal {
                                            content.push(format!(
                        "impl<'a> {connections_mod}::Signal for {}<'a> {{}}\n",
                        receiver.type_name,
                        connections_mod = connections_mod
                      ));
                                        }
                                        let doc = format_doc(
                                            &doc_formatter::doc_for_qt_builtin_receiver_method(
                                                cpp_type_name,
                                                receiver,
                                            ),
                                        );
                                        struct_content.push(format!(
                                            "\
{doc}pub fn {method_name}(&self) -> {type_name} {{
  {type_name}(self.0)
}}\n",
                                            type_name = receiver.type_name,
                                            method_name = receiver.method_name,
                                            doc = doc,
                                        ));
                                    }
                                }
                                content.push(format!(
                                    "impl<'a> {}<'a> {{\n{}\n}}\n",
                                    struct_type,
                                    struct_content.join("")
                                ));
                                type_impl_content.push(format!(
                                    "\
{doc}pub fn {struct_method}(&self) -> {struct_type} {{
  {struct_type}(self)
}}\n",
                                    struct_method = struct_method,
                                    struct_type = struct_type,
                                    doc = format_doc(struct_method_doc)
                                ));
                            }
                        }
                        content.push(format!(
                            "impl {} {{\n{}\n}}\n",
                            obj_name,
                            type_impl_content.join("")
                        ));
                        results.push(format!(
              "/// Types for accessing built-in Qt signals and slots present in this module\n\
               pub mod connection {{\n{}\n}}\n\n",
              content.join("")
            ));
                    }
                }
                RustTypeDeclarationKind::MethodParametersTrait {
                    ref shared_arguments,
                    ref impls,
                    ref lifetime,
                    ref common_return_type,
                    ref is_unsafe,
                    ..
                } => {
                    let arg_list = self
                        .arg_texts(shared_arguments, lifetime.as_ref())
                        .join(", ");
                    let trait_lifetime_specifier = match *lifetime {
                        Some(ref lf) => format!("<'{}>", lf),
                        None => String::new(),
                    };
                    if impls.is_empty() {
                        return Err("MethodParametersTrait with empty impls".into());
                    }
                    let return_type_decl = if common_return_type.is_some() {
                        ""
                    } else {
                        "type ReturnType;"
                    };
                    let return_type_string =
                        if let Some(ref common_return_type) = *common_return_type {
                            self.rust_type_to_code(common_return_type)
                        } else {
                            "Self::ReturnType".to_string()
                        };
                    let maybe_unsafe = if *is_unsafe { "unsafe " } else { "" };
                    results.push(format!(
                        "pub trait {name}{trait_lifetime_specifier} {{\n\
              {return_type_decl}\n\
              {maybe_unsafe}fn exec(self, {arg_list}) -> {return_type_string};
            }}",
                        name = type1.name.last_name()?,
                        maybe_unsafe = maybe_unsafe,
                        arg_list = arg_list,
                        trait_lifetime_specifier = trait_lifetime_specifier,
                        return_type_decl = return_type_decl,
                        return_type_string = return_type_string
                    ));
                    for variant in impls {
                        let final_lifetime = if lifetime.is_none()
                            && (variant
                                .arguments
                                .iter()
                                .any(|t| t.argument_type.rust_api_type.is_ref())
                                || variant.return_type.rust_api_type.is_ref())
                        {
                            Some("a".to_string())
                        } else {
                            lifetime.clone()
                        };
                        let lifetime_specifier = match final_lifetime {
                            Some(ref lf) => format!("<'{}>", lf),
                            None => String::new(),
                        };
                        let final_arg_list = self
                            .arg_texts(shared_arguments, final_lifetime.as_ref())
                            .join(", ");
                        let tuple_item_types: Vec<_> = variant
                            .arguments
                            .iter()
                            .map(|t| {
                                if let Some(ref lifetime) = final_lifetime {
                                    self.rust_type_to_code(
                                        &t.argument_type
                                            .rust_api_type
                                            .with_lifetime(lifetime.to_string()),
                                    )
                                } else {
                                    self.rust_type_to_code(&t.argument_type.rust_api_type)
                                }
                            })
                            .collect();
                        let mut tmp_vars = Vec::new();
                        if variant.arguments.len() == 1 {
                            tmp_vars.push(format!("let {} = self;", variant.arguments[0].name));
                        } else {
                            for (index, arg) in variant.arguments.iter().enumerate() {
                                tmp_vars.push(format!("let {} = self.{};", arg.name, index));
                            }
                        }
                        let return_type_string = match final_lifetime {
                            Some(ref lifetime) => self.rust_type_to_code(
                                &variant
                                    .return_type
                                    .rust_api_type
                                    .with_lifetime(lifetime.to_string()),
                            ),
                            None => self.rust_type_to_code(&variant.return_type.rust_api_type),
                        };
                        let return_type_decl = if common_return_type.is_some() {
                            String::new()
                        } else {
                            format!("type ReturnType = {};", return_type_string)
                        };
                        results.push(format!(
                            include_str!("../templates/crate/impl_overloading_trait.rs.in"),
                            maybe_unsafe = maybe_unsafe,
                            lifetime_specifier = lifetime_specifier,
                            trait_lifetime_specifier = trait_lifetime_specifier,
                            trait_name = type1.name.last_name()?,
                            final_arg_list = final_arg_list,
                            impl_type = if tuple_item_types.len() == 1 {
                                tuple_item_types[0].clone()
                            } else {
                                format!("({})", tuple_item_types.join(","))
                            },
                            return_type_decl = return_type_decl,
                            return_type_string = return_type_string,
                            tmp_vars = tmp_vars.join("\n"),
                            body = self.generate_ffi_call(variant, shared_arguments, *is_unsafe)?
                        ));
                    }
                }
            };
        }
        for method in &data.functions {
            results.push(self.generate_rust_final_function(method)?);
        }
        results.push(self.generate_trait_impls(&data.trait_impls)?);
        for submodule in &data.submodules {
            let submodule_doc = submodule
                .doc
                .as_ref()
                .map(|d| format_doc(d))
                .unwrap_or_default();
            results.push(format!(
                "{}pub mod {} {{\n{}}}\n\n",
                submodule_doc,
                submodule.name,
                self.generate_module_code(submodule)?
            ));
            for type1 in &submodule.types {
                if let RustTypeDeclarationKind::CppTypeWrapper { ref kind, .. } = type1.kind {
                    if let RustTypeWrapperKind::Struct {
                        ref slot_wrapper, ..
                    } = *kind
                    {
                        if let Some(ref slot_wrapper) = *slot_wrapper {
                            let arg_texts: Vec<_> = slot_wrapper
                                .arguments
                                .iter()
                                .map(|t| self.rust_type_to_code(&t.rust_api_type))
                                .collect();
                            let cpp_args = slot_wrapper
                                .arguments
                                .iter()
                                .map(|t| t.cpp_type.to_cpp_pseudo_code())
                                .join(", ");
                            let args = arg_texts.join(", ");
                            let args_tuple =
                                format!("{}{}", args, if arg_texts.len() == 1 { "," } else { "" });
                            let connections_mod = RustName::new(vec![
                                "qt_core".to_string(),
                                "connection".to_string(),
                            ])?
                            .full_name(Some(&self.config.crate_properties.name()));
                            let object_type_name = RustName::new(vec![
                                "qt_core".to_string(),
                                "object".to_string(),
                                "Object".to_string(),
                            ])?
                            .full_name(Some(&self.config.crate_properties.name()));
                            let callback_args = slot_wrapper
                                .arguments
                                .iter()
                                .enumerate()
                                .map(|(num, t)| {
                                    format!(
                                        "arg{}: {}",
                                        num,
                                        self.rust_type_to_code(&t.rust_ffi_type)
                                    )
                                })
                                .join(", ");
                            let func_args = slot_wrapper
                                .arguments
                                .iter()
                                .enumerate()
                                .map_if_ok(|(num, t)| {
                                    self.convert_type_from_ffi(
                                        t,
                                        format!("arg{}", num),
                                        false,
                                        false,
                                    )
                                })?
                                .join(", ");
                            results.push(format!(
                                include_str!("../templates/crate/closure_slot_wrapper.rs.in"),
                                type_name = type1
                                    .name
                                    .full_name(Some(&self.config.crate_properties.name())),
                                pub_type_name = slot_wrapper.public_type_name,
                                callback_name = slot_wrapper.callback_name,
                                args = args,
                                args_tuple = args_tuple,
                                connections_mod = connections_mod,
                                object_type_name = object_type_name,
                                func_args = func_args,
                                callback_args = callback_args,
                                cpp_args = cpp_args
                            ));
                        }
                    }
                }
            }
        }
        Ok(results.join(""))
    }

    /// Runs `rustfmt` on a Rust file `path`.
    fn call_rustfmt(&self, path: &PathBuf) {
        let result = ::std::panic::catch_unwind(|| {
            rustfmt::format_input(
                rustfmt::Input::File(path.clone()),
                &self.rustfmt_config,
                Some(&mut ::std::io::stdout()),
            )
        });
        match result {
            Ok(rustfmt_result) => {
                if rustfmt_result.is_err() {
                    log::error(format!("rustfmt returned Err on file: {:?}", path));
                }
            }
            Err(cause) => {
                log::error(format!("rustfmt paniced on file: {:?}: {:?}", path, cause));
            }
        }
        assert!(path.as_path().is_file());
    }

    /// Creates a top level module file.
    pub fn generate_module_file(&self, data: &RustModule) -> Result<()> {
        let mut file_path = self.config.output_path.clone();
        file_path.push("src");
        file_path.push(format!("{}.rs", &data.name));
        self.save_src_file(&file_path, &self.generate_module_code(data)?)?;
        self.call_rustfmt(&file_path);
        Ok(())
    }

    /// Generates `ffi.in.rs` file.
    pub fn generate_ffi_file(&self, functions: &[(String, Vec<RustFFIFunction>)]) -> Result<()> {
        let mut code = String::new();
        code.push_str("extern \"C\" {\n");
        for &(ref include_file, ref functions) in functions {
            code.push_str(&format!("  // Header: {}\n", include_file));
            for function in functions {
                code.push_str(&self.rust_ffi_function_to_code(function));
            }
            code.push_str("\n");
        }
        code.push_str("}\n");

        let src_dir_path = self.config.output_path.join("src");
        let file_path = src_dir_path.join("ffi.in.rs");
        self.save_src_file(&file_path, &code)?;
        // no rustfmt for ffi file
        Ok(())
    }

    /// Creates new Rust source file or merges it with the existing file.
    fn save_src_file(&self, path: &Path, code: &str) -> Result<()> {
        const INCLUDE_GENERATED_MARKER: &'static str = "include_generated!();";
        const CPP_LIB_VERSION_MARKER: &'static str = "{cpp_to_rust.cpp_lib_version}";
        if path.exists() {
            let mut template = file_to_string(path)?;
            if template.contains(CPP_LIB_VERSION_MARKER) {
                if let Some(ref cpp_lib_version) = self.config.cpp_lib_version {
                    template = template.replace(CPP_LIB_VERSION_MARKER, cpp_lib_version);
                } else {
                    return Err("C++ library version was not set in configuration.".into());
                }
            }
            if let Some(index) = template.find(INCLUDE_GENERATED_MARKER) {
                let mut file = create_file(&path)?;
                file.write(&template[0..index])?;
                file.write(code)?;
                file.write(&template[index + INCLUDE_GENERATED_MARKER.len()..])?;
            } else {
                let name = os_str_to_str(
                    path.file_name()
                        .with_context(|| unexpected("no file name in path"))?,
                )?;
                let e = format!(
          "Generated source file {} conflicts with the crate template. \
           Use \"include_generated!();\" macro in the crate template to merge files or block \
           items of this module in the generator's configuration.",
          name
        );
                return Err(e.into());
            }
        } else {
            let mut file = create_file(&path)?;
            file.write(code)?;
        }
        Ok(())
    }
}

use common::errors::{unexpected, Result, ResultExt};
use common::file_utils::{
    copy_file, copy_recursively, create_dir_all, create_file, file_to_string, os_str_to_str,
    path_to_str, read_dir, repo_crate_local_path, save_toml, PathBufWithAdded,
};
use common::log;
use common::string_utils::{CaseOperations, JoinWithSeparator};
use common::utils::MapIfOk;
use doc_formatter;
use rust_generator::RustGeneratorOutput;
use rust_info::{
    DependencyInfo, RustFFIFunction, RustFunction, RustFunctionArgument, RustFunctionArguments,
    RustFunctionArgumentsVariant, RustFunctionScope, RustModule, RustQtReceiverType,
    RustTypeDeclarationKind, RustTypeWrapperKind, TraitImpl, TraitImplExtra,
};
use rust_type::{CompleteType, RustName, RustToFfiTypeConversion, RustType, RustTypeIndirection};
use std::path::{Path, PathBuf};

use common::toml;
use rustfmt;
use versions;

use config::CrateProperties;

/// Data required for Rust code generation.
pub struct RustCodeGeneratorConfig<'a> {
    /// Crate properties, as in `Config`.
    pub crate_properties: CrateProperties,
    /// Path to the generated crate's root.
    pub output_path: PathBuf,
    /// Path to the crate template, as in `Config`.
    /// May be `None` if it wasn't set in `Config`.
    pub crate_template_path: Option<PathBuf>,
    /// Name of the C++ wrapper library.
    pub cpp_ffi_lib_name: String,
    /// Version of the original C++ library.
    pub cpp_lib_version: Option<String>,
    /// `cpp_to_rust` based dependencies of the generated crate.
    pub generator_dependencies: &'a [DependencyInfo],
    /// As in `Config`.
    pub write_dependencies_local_paths: bool,
}

*/
