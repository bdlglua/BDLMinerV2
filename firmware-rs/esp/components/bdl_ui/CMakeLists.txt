# M5GFX's board detection and panel drivers for the Rust firmware's M5Stack
# image (feature `m5stack`), driven from src/display.rs through the C
# functions of ui_ffi.cpp. The renderer is Rust (bdlminer_app::ui_view);
# the other boards' panels are Rust too (src/lcd.rs). Built for every
# board, linked only into the M5Stack image.
idf_component_register(
    SRCS "ui_ffi.cpp"
    INCLUDE_DIRS "include"
    PRIV_REQUIRES driver)
