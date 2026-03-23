use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::str::FromStr;
use syn::{
    Data, DeriveInput, Field, Fields, Meta, Token, parse_macro_input, punctuated::Punctuated,
};

use convert_case::ccase;

struct MCPMeta {
    title: Option<String>,
    description: Option<String>,
}

fn classify_inner(ty: &syn::Type, is_array: bool, is_optional: bool) -> (&'static str, bool, bool) {
    if let syn::Type::Path(type_path) = ty {
        let last = type_path.path.segments.last().unwrap();
        let ident = last.ident.to_string();

        match ident.as_str() {
            // Unwrap Option — mark optional, recurse
            "Option" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return classify_inner(inner, is_array, true);
                    }
                }
            }
            // Unwrap Vec/HashSet — mark array, recurse into item type
            "Vec" | "HashSet" | "BTreeSet" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return classify_inner(inner, true, is_optional);
                    }
                }
            }
            // Unwrap Box/Arc/Rc — transparent, recurse
            "Box" | "Arc" | "Rc" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return classify_inner(inner, is_array, is_optional);
                    }
                }
            }
            // HashMap/BTreeMap — always an object
            "HashMap" | "BTreeMap" => return ("object", is_array, is_optional),

            // Base types
            "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "usize" | "isize" => {
                return ("integer", is_array, is_optional);
            }
            "f32" | "f64" => return ("number", is_array, is_optional),
            "bool" => return ("boolean", is_array, is_optional),
            "String" | "str" => return ("string", is_array, is_optional),
            _ => return ("object", is_array, is_optional),
        }
    }
    ("object", is_array, is_optional)
}

fn type_to_json(name: &String, f: &Field) -> proc_macro2::TokenStream {
    let (jt, is_array, _) = classify_inner(&f.ty, false, false);

    if is_array {
        quote! {
            #name: {
                "type": "array",
                "items": {
                    "type": #jt
                }
            }
        }
    } else {
        quote! {
            #name: {
                "type": #jt
            }
        }
    }
}

fn parse_register_meta(ast: &DeriveInput) -> MCPMeta {
    let mut title = None;
    let mut description = None;

    for attr in &ast.attrs {
        if !attr.path().is_ident("meta") {
            continue;
        }

        // Parse the contents as a comma-separated list of name = value pairs
        let nested = attr
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .unwrap_or_default();

        for meta in nested {
            match meta {
                Meta::NameValue(nv) => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        if nv.path.is_ident("title") {
                            title = Some(s.value());
                        } else if nv.path.is_ident("description") {
                            description = Some(s.value());
                        }
                    }
                }
                _ => (),
            }
        }
    }

    MCPMeta { title, description }
}

fn generic_derive(dstruct: String, info_type: String, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let meta = parse_register_meta(&ast);

    let name = &ast.ident;
    let name_s = name.to_string();

    let ast_d = match &ast.data {
        Data::Struct(f) => f,
        _ => {
            return syn::Error::new_spanned(
                &ast.ident,
                format!("{} can only be derived on structs", dstruct),
            )
            .to_compile_error()
            .into();
        }
    };

    let params = match &ast_d.fields {
        Fields::Named(f) => f
            .named
            .iter()
            .map(|f| (f.ident.as_ref().unwrap().to_string(), f))
            .collect(),
        _ => vec![],
    };

    let snek = ccase!(snake, name_s.clone());
    let xn = format_ident!("{}", dstruct);
    let ityp = format_ident!("{}", info_type);
    let mtitle = meta.title.unwrap_or(name_s.clone());
    let mdescription = meta.description.unwrap_or(name_s.clone());
    let required: Vec<String> = params
        .iter()
        .filter_map(|(x, fld)| {
            if let syn::Type::Path(t) = &fld.ty
                && t.path
                    .segments
                    .first()
                    .map(|s| s.ident != "Option")
                    .unwrap_or(true)
            {
                Some(x.clone())
            } else {
                None
            }
        })
        .collect();

    let prop_toks: Vec<proc_macro2::TokenStream> = params
        .iter()
        .map(|(nm, f)| type_to_json(&nm, &(*f).clone()))
        .collect();

    let expanded = quote! {
        impl registry::#xn for #name {
            fn info() -> ::registry::Info {
                ::registry::Info {
                    name: #name_s,
                    info_type: ::registry::InfoType::#ityp,
                    params: #name::params,
                }
            }

            fn params() -> Value {
                json!({
    "name": #snek,
    "title": #mtitle,
    "description": #mdescription,
    "inputSchema": {
        "type": "object",
        "properties": {#(#prop_toks),*},
        "required": [#(#required),*] //...
    },
                })
            }

            fn from_args(v: Map<String, Value>) -> Result<Self, String> {
                match serde_json::from_value(Value::Object(v)) {
                    Ok(a) => Ok(a),
                    Err(e) => Err(format!("{}", e)),
                }
            }
        }

        ::registry::_i::submit! {
            ::registry::Info {
                name: #name_s,
                info_type: ::registry::InfoType::#ityp,
                params: #name::params,
            }
        }
    };

    println!("{}", expanded.to_string());

    expanded.into()
}

#[proc_macro_derive(MCPTool, attributes(meta))]
pub fn derive_mcp_tool(input: TokenStream) -> TokenStream {
    generic_derive("MCPTool".to_string(), "TOOL".to_string(), input)
}

#[proc_macro_derive(MCPResource, attributes(meta))]
pub fn derive_mcp_resource(input: TokenStream) -> TokenStream {
    generic_derive("MCPResource".to_string(), "RESOURCE".to_string(), input)
}
