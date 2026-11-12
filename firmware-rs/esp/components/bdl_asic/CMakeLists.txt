# The C++ firmware's ASIC drivers (src/gamma601.cpp for the Bitaxe Gamma 601,
# src/nerdnos.cpp for the NerdNOS), built unchanged and driven from Rust
# (src/asic.rs) through asic_ffi.cpp. stub/ supplies the few Arduino calls
# they make (UART, GPIO, ADC, Preferences) on ESP-IDF. Both boards are
# ESP32-S3s: on other chips the component is empty.
if(NOT CONFIG_IDF_TARGET_ESP32S3)
    idf_component_register()
    return()
endif()

set(cpp "${CMAKE_CURRENT_LIST_DIR}/../../../../src")

idf_component_register(
    SRCS "arduino_shim.cpp" "asic_ffi.cpp" "gamma_ffi.cpp" "nerdnos_ffi.cpp"
         "${cpp}/gamma601.cpp" "${cpp}/nerdnos.cpp" "${cpp}/core/bm1370.cpp" "${cpp}/core/bm1397.cpp"
    INCLUDE_DIRS "include"
    PRIV_INCLUDE_DIRS "${cpp}" "stub"
    PRIV_REQUIRES bdl_hwsha driver esp_timer esp_adc nvs_flash)

# The board each source is built for, as the C++ firmware's environments
# define it (settings.h and miner.h depend on them).
set_source_files_properties("asic_ffi.cpp" PROPERTIES COMPILE_DEFINITIONS "BOARD_ASIC")
set_source_files_properties("gamma_ffi.cpp" "${cpp}/gamma601.cpp"
    PROPERTIES COMPILE_DEFINITIONS "BOARD_GAMMA601;BOARD_ASIC")
set_source_files_properties("nerdnos_ffi.cpp" "${cpp}/nerdnos.cpp"
    PROPERTIES COMPILE_DEFINITIONS "BOARD_NERDNOS;BOARD_ASIC")
target_compile_options(${COMPONENT_LIB} PRIVATE -Wno-missing-field-initializers)
