use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, AttrStyle, Attribute, Data, DataEnum, DataStruct,
    DeriveInput, Field, Fields, FieldsNamed, Ident, LitStr, Variant,
};

type Result<T> = std::result::Result<T, syn::Error>;

#[proc_macro_derive(PropertyList)]
pub fn property_list(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);
    match impl_proplist(&derive_input) {
        Ok(props) => props.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

#[proc_macro_derive(Backend)]
pub fn backend(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);
    match impl_backends(&derive_input) {
        Ok(props) => props.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn impl_proplist(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    match input.data {
        Data::Struct(DataStruct {
            struct_token: _,
            ref fields,
            semi_token: _,
        }) => {
            let insert_props = impl_insert_props(false, fields)?;
            Ok(quote! {
                impl cmdstruct::Arg for #ident {

                    fn append_arg(&self, command: &mut std::process::Command) {
                        #insert_props
                        command.arg(&format!("{props}"));
                    }

                }
            }
            .into())
        }
        _ => Err(syn::Error::new(input.span(), "Only structs are supported.")),
    }
}

fn field_identifiers(fields: &Fields) -> Result<Vec<Ident>> {
    match &fields {
        Fields::Named(FieldsNamed {
            brace_token: _,
            ref named,
        }) => Ok(named
            .clone()
            .iter()
            .filter_map(|field| field.ident.clone())
            .collect()),
        Fields::Unit => Ok(Vec::new()),
        Fields::Unnamed(_) => Err(syn::Error::new(
            fields.span(),
            "Unnamed fields are not supported.",
        )),
    }
}

fn backend_name_matcher(tuple: (&Ident, &Variant)) -> Result<proc_macro2::TokenStream> {
    let ident = &tuple.1.ident;
    let enum_ident = &tuple.0;
    let name = format!("{ident}").to_lowercase();
    let fields = field_identifiers(&tuple.1.fields)?;
    let enum_fields = if fields.is_empty() {
        quote! {}
    } else {
        quote! { { #(#fields: _, )* } }
    };
    Ok(quote! {
        #enum_ident::#ident #enum_fields => #name,
    })
}

fn backend_properties_matcher(tuple: (&Ident, &Variant)) -> Result<proc_macro2::TokenStream> {
    let ident = &tuple.1.ident;
    let insert_props = impl_insert_props(true, &tuple.1.fields)?;
    let enum_ident = &tuple.0;
    let fields = field_identifiers(&tuple.1.fields)?;
    let enum_fields = if fields.is_empty() {
        quote! {}
    } else {
        quote! { { #(#fields, )* } }
    };
    let matcher = quote! {
        #enum_ident::#ident #enum_fields => {
            #insert_props
            props
        }
    };
    Ok(matcher)
}

fn impl_backends(input: &DeriveInput) -> Result<TokenStream> {
    let ident = &input.ident;
    match input.data {
        Data::Enum(DataEnum {
            enum_token: _,
            brace_token: _,
            ref variants,
        }) => {
            let name_matches: Vec<_> = variants
                .iter()
                .map(|variant| (ident, variant))
                .map(backend_name_matcher)
                .collect::<Result<Vec<_>>>()?;
            let properties_matches: Vec<proc_macro2::TokenStream> = variants
                .iter()
                .map(|variant| (ident, variant))
                .map(backend_properties_matcher)
                .collect::<Result<Vec<_>>>()?;
            Ok(quote! {
                impl crate::qemu::args::Backend for #ident {

                    fn name(&self) -> &str {
                        match self {
                            #(#name_matches)*
                        }
                    }

                    fn properties<'backend>(&'backend self)
                        -> crate::qemu::args::PropertyList<'backend> {
                            match self {
                                #(#properties_matches, )*
                            }
                    }

                }
            }
            .into())
        }
        _ => Err(syn::Error::new(input.span(), "Only structs are supported.")),
    }
}

enum SerdeAttribute {
    Flatten,
    Rename(Ident),
}

fn insert_prop(local: bool, field: &Field) -> Option<proc_macro2::TokenStream> {
    if let Some(ref ident) = field.ident {
        let value = if local {
            quote! { #ident }
        } else {
            quote! { &self.#ident }
        };
        let tokens = match parse_attributes(&field.attrs) {
            Some(SerdeAttribute::Flatten) => quote! {
                for (key, value) in #value {
                    props.insert(key, value);
                }
            },
            Some(SerdeAttribute::Rename(ref rename)) => {
                let name_str = format!("{rename}");
                quote! {
                    props.insert(#name_str, #value);
                }
            }
            None => {
                let name_str = format!("{ident}");
                quote! {
                    props.insert(#name_str, #value);
                }
            }
        };
        Some(tokens)
    } else {
        None
    }
}

fn impl_insert_props(local: bool, fields: &Fields) -> Result<proc_macro2::TokenStream> {
    match fields {
        Fields::Named(FieldsNamed {
            brace_token: _,
            ref named,
        }) => {
            let insert_props: Vec<proc_macro2::TokenStream> = named
                .iter()
                .filter_map(|field| insert_prop(local, field))
                .collect();
            Ok(quote! {
                let mut props = crate::qemu::args::PropertyList::default();
                #(#insert_props)*
            })
        }
        Fields::Unit => Ok(quote! {
            let props = crate::qemu::args::PropertyList::default();
        }),
        _ => Err(syn::Error::new(
            fields.span(),
            "Unnamed fields are not supported",
        )),
    }
}

/// Check if a flattened serde attribute
fn parse_attributes(attrs: &[Attribute]) -> Option<SerdeAttribute> {
    let mut flatten = false;
    let mut rename = None;
    for attr in attrs {
        match attr.style {
            AttrStyle::Outer => match &attr.meta {
                syn::Meta::List(list) if list.path.is_ident("serde") => {
                    let _ = attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("flatten") {
                            flatten = true;
                        } else if meta.path.is_ident("rename") {
                            let value = meta.value()?;
                            let s: LitStr = value.parse()?;
                            rename = Some(Ident::new(&s.value(), attr.span()));
                        }
                        Ok(())
                    });
                }
                _ => {}
            },
            _ => {}
        };
    }
    if flatten {
        Some(SerdeAttribute::Flatten)
    } else if let Some(rename) = rename {
        Some(SerdeAttribute::Rename(rename))
    } else {
        None
    }
}
