# The C++ firmware's display renderer (src/ui_view.cpp, src/ui_font.cpp and
# its fonts), built unchanged on M5GFX and driven from Rust (src/display.rs)
# through the C functions of ui_ffi.cpp. Only boards with a screen use it.
set(cpp "${CMAKE_CURRENT_LIST_DIR}/../../../../src")

idf_component_register(
    SRCS "ui_ffi.cpp" "${cpp}/ui_view.cpp" "${cpp}/ui_font.cpp"
    INCLUDE_DIRS "include"
    PRIV_INCLUDE_DIRS "${cpp}"
    PRIV_REQUIRES driver)

# The renderer without the Arduino board layer (as tools/preview_display.py
# builds it).
target_compile_definitions(${COMPONENT_LIB} PRIVATE BDLMINER_DISPLAY_PREVIEW=1)
