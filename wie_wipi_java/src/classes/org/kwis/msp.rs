pub mod db;
pub mod handset;
pub mod io;
pub mod lcdui;
// Every method here is a JVM bridge, so its parameters are the Java method's
// own plus the three the bridge always carries. `configure(IIIII)V` is eight
// arguments because the handset's Component.configure takes five, not because
// anything here chose to pass that many.
#[allow(clippy::too_many_arguments)]
pub mod lwc;
pub mod media;
