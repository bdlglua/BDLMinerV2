# The C++ firmware's hardware SHA-256 engine and Xtensa assembly kernels,
# built unchanged from ../../../../src and called from Rust (src/hwsha.rs)
# through the C functions of hwsha_ffi.cpp.
set(cpp "${CMAKE_CURRENT_LIST_DIR}/../../../../src")

set(srcs
    "hwsha_ffi.cpp"
    "${cpp}/hw_sha.cpp"
    "${cpp}/core/sha256.cpp"
    "${cpp}/core/work.cpp"
    "${cpp}/core/fill.cpp")
if(CONFIG_IDF_TARGET_ESP32)
    list(APPEND srcs "${cpp}/hw_sha_classic.S" "${cpp}/hw_sha_fill.S")
endif()
if(CONFIG_IDF_TARGET_ESP32 OR CONFIG_IDF_TARGET_ESP32S3)
    list(APPEND srcs "${cpp}/sw_sha_prefilter.S")
endif()

idf_component_register(
    SRCS ${srcs}
    INCLUDE_DIRS "include"
    PRIV_INCLUDE_DIRS "${cpp}" "stub"
    PRIV_REQUIRES mbedtls esp_timer driver esp_hw_support)

# The kernels are built for speed, as in the C++ firmware.
target_compile_options(${COMPONENT_LIB} PRIVATE -O2)
