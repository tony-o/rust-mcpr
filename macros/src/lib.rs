use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::str::FromStr;
use syn::{
    Data, DeriveInput, Field, Fields, Meta, Token, parse_macro_input, punctuated::Punctuated,
};

fn classify_inner(ty: &syn::Type, is_array: bool, is_optional: bool) -> (&'static str, bool, bool) {
    if let syn::Type::Path(type_path) = ty {
        let last = type_path.path.segments.last().unwrap();
        let ident = last.ident.to_string();

        match ident.as_str() {
            // Unwrap Option — mark optional, recurse
            "Option" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                {
                    return classify_inner(inner, is_array, true);
                }
            }
            // Unwrap Vec/HashSet — mark array, recurse into item type
            "Vec" | "HashSet" | "BTreeSet" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                {
                    return classify_inner(inner, true, is_optional);
                }
            }
            // Unwrap Box/Arc/Rc — transparent, recurse
            "Box" | "Arc" | "Rc" => {
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                {
                    return classify_inner(inner, is_array, is_optional);
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

fn type_to_json(name: &str, f: &Field) -> proc_macro2::TokenStream {
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct MetaIconish {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct Metaish {
    pub title: Option<String>,
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub icons: Option<Vec<MetaIconish>>,
}

fn parse_register_meta(ast: &DeriveInput) -> Result<Metaish, String> {
    let mut title = None;
    let mut description = None;
    let mut icons = None;
    let mut uri = "unset:///".to_string();
    let mut mime_type = None;
    let mut name = ast.ident.to_string();

    for attr in &ast.attrs {
        if !attr.path().is_ident("meta") {
            continue;
        }

        // Parse the contents as a comma-separated list of name = value pairs
        let nested = attr
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .unwrap_or_default();

        for meta in nested {
            if let Meta::NameValue(nv) = meta
                && let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
            {
                if nv.path.is_ident("title") {
                    title = Some(s.value());
                } else if nv.path.is_ident("description") {
                    description = Some(s.value());
                } else if nv.path.is_ident("icons") {
                    match serde_json::from_str::<Vec<MetaIconish>>(&s.value()) {
                        Ok(a) => icons = Some(a),
                        Err(e) => {
                            return Err(format!(
                                "failed to parse icons for {}, this must be valid json\n{}",
                                &ast.ident, e
                            ));
                        }
                    }
                } else if nv.path.is_ident("name") {
                    name = s.value();
                } else if nv.path.is_ident("uri") {
                    uri = s.value();
                } else if nv.path.is_ident("mime_type") {
                    mime_type = Some(s.value());
                }
            }
        }
    }

    Ok(Metaish {
        title,
        name,
        description,
        uri,
        mime_type,
        icons,
    })
}

fn generic_derive(dstruct: String, info_type: String, input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let meta = match parse_register_meta(&ast) {
        Ok(m) => m,
        Err(e) => {
            return syn::Error::new_spanned(&ast.ident, e)
                .to_compile_error()
                .into();
        }
    };

    let name = &ast.ident;
    let name_s = name.to_string();

    if info_type == "Resource" && meta.uri == "unset:///" {
        return syn::Error::new_spanned(
            &ast.ident,
            format!("Resource {} must have a #[meta(uri = ...) component", name),
        )
        .to_compile_error()
        .into();
    }
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

    //let snek = ccase!(snake, name_s.as_str());
    let xn = format_ident!("{}", dstruct);
    let ityp = format_ident!("{}", info_type);
    let mtitle = meta.title.clone().unwrap_or(name_s.clone());
    let mdescription = meta.description.clone().unwrap_or(name_s.clone());
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
        .map(|(nm, f)| type_to_json(nm.as_str(), f))
        .collect();

    let from_args_rval = if info_type == "Tool" {
        quote! { ::mcp_router::registry::FromArgResult::Tool(Box::new(a)) }
    } else {
        quote! { ::mcp_router::registry::FromArgResult::Resource(Box::new(a)) }
    };

    let meta_title = if let Some(t) = meta.title.clone() {
        quote! { Some(#t.to_string()) }
    } else {
        quote! { None }
    };
    let meta_description = if let Some(t) = meta.description.clone() {
        quote! { Some(#t.to_string()) }
    } else {
        quote! { None }
    };
    let meta_uri = meta.uri;
    let meta_name = meta.name;
    let meta_mime_type = if let Some(t) = meta.mime_type {
        quote! { Some(#t.to_string()) }
    } else {
        quote! { None }
    };
    let meta_icons = if let Some(t) = meta.icons {
        let j = match proc_macro2::TokenStream::from_str(
            match serde_json::to_string(&t) {
                Ok(a) => {
                    format!("serde_json::json!({})", a)
                }
                Err(e) => {
                    return syn::Error::new_spanned(
                        &ast.ident,
                        format!("{} failed to serialize the \"icons\" meta: {}", dstruct, e),
                    )
                    .to_compile_error()
                    .into();
                }
            }
            .as_str(),
        ) {
            Ok(t) => t,
            Err(_e) => {
                return syn::Error::new_spanned(
                    &ast.ident,
                    format!("{} failed to tokenize the \"icons\" meta", dstruct),
                )
                .to_compile_error()
                .into();
            }
        };
        quote! { Some(#j) }
    } else {
        quote! { None }
    };

    let executor_class = format_ident!(
        "MCP{}Executor",
        if info_type == "Tool" {
            "Tool"
        } else {
            "Resource"
        }
    );

    let resource_extras = if info_type == "Tool" {
        quote! {
            is_template: || false,
            serves: |_| false,
        }
    } else {
        quote! {
            is_template: #name::is_template,
            serves: #name::serves,
        }
    };

    let expanded = quote! {
        impl ::mcp_router::registry::#xn for #name {
            fn params() -> Value {
                serde_json::json!({
                    "name": #meta_name,
                    "title": #mtitle,
                    "description": #mdescription,
                    "inputSchema": {
                        "type": "object",
                        "properties": {#(#prop_toks),*},
                        "required": [#(#required),*]
                    },
                })
            }

            fn meta() -> Vec<::mcp_router::registry::MCPMeta> {
                vec![::mcp_router::registry::MCPMeta {
                    title: #meta_title,
                    description: #meta_description,
                    uri: #meta_uri.to_string(),
                    mime_type: #meta_mime_type,
                    icons: #meta_icons,
                    name: #meta_name.to_string(),
                }]
            }

            fn from_args(v: &serde_json::Value) -> ::mcp_router::registry::FromArgResult {
                match serde_json::from_value::<Self>(v.clone()) {
                    Ok(a) => #from_args_rval,
                    Err(e) => ::mcp_router::registry::FromArgResult::Error(format!("{}", e)),
                }
            }

            fn get_executor(&self) -> &dyn #executor_class { self }

        }

        ::mcp_router::registry::_i::submit! {
            ::mcp_router::registry::Info {
                name: #meta_name,
                info_type: ::mcp_router::registry::InfoType::#ityp,
                params: #name::params,
                from_args: #name::from_args,
                meta: #name::meta,
                #resource_extras
            }
        }
    };

    expanded.into()
}

#[proc_macro_derive(MCPTool, attributes(meta))]
pub fn derive_mcp_tool(input: TokenStream) -> TokenStream {
    generic_derive("MCPTool".to_string(), "Tool".to_string(), input)
}

#[proc_macro_derive(MCPResource, attributes(meta))]
pub fn derive_mcp_resource(input: TokenStream) -> TokenStream {
    generic_derive("MCPResource".to_string(), "Resource".to_string(), input)
}
