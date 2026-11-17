//! Gives the Windows program its icon, so Explorer and the taskbar show the
//! MiniMobile tile rather than a blank executable. Other targets have no
//! resource to embed - SDL takes the window icon at run time instead.

fn main() {
    println!("cargo:rerun-if-changed=assets/minimobile.rc");
    println!("cargo:rerun-if-changed=assets/minimobile.ico");
    embed_resource::compile("assets/minimobile.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap_or_else(|error| panic!("could not embed the Windows icon: {error}"));
}
