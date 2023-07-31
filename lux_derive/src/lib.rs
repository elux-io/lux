use proc_macro::TokenStream;
use quote::quote;
use syn;

#[proc_macro_derive(HotReload)]
pub fn hot_reload_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_macro(&ast)
}

fn impl_macro(ast: &syn::DeriveInput) -> TokenStream {
    let app_name = &ast.ident;

    let app_functions = quote! {
        #[no_mangle]
        fn lux_app_new(window: &Window) -> *mut std::ffi::c_void {
            Box::into_raw(Box::new(<#app_name>::new(window))) as *mut std::ffi::c_void
        }

        #[no_mangle]
        unsafe fn lux_app_drop(app: *mut std::ffi::c_void) {
            drop(Box::from_raw(app as *mut #app_name));
        }

        #[no_mangle]
        unsafe fn lux_app_update(app: *mut std::ffi::c_void) {
            (app as *mut #app_name).as_mut().unwrap().update();
        }

        #[no_mangle]
        unsafe fn lux_app_on_resize(app: *mut std::ffi::c_void, width: u32, height: u32) {
            (app as *mut #app_name).as_mut().unwrap().on_resize(width, height);
        }
    };

    app_functions.into()
}
