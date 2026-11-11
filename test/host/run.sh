#!/bin/sh
# Host tests for the portable mining core.
#   ./test/host/run.sh                 unit tests (offline)
#   ./test/host/run.sh pool [args...]  live test against a Stratum pool
set -e
cd "$(dirname "$0")/../.."
JSON=.pio/libdeps/esp32-headless/ArduinoJson/src
[ -d "$JSON" ] || pio pkg install -e esp32-headless >/dev/null
mkdir -p .pio/host
CORE="src/core/sha256.cpp src/core/work.cpp src/core/stratum.cpp src/core/address.cpp src/core/fill.cpp"
if [ "$1" = "pool" ]; then
    shift
    g++ -std=c++17 -O2 -Wall -pthread -Isrc -I"$JSON" test/host/pool_test.cpp $CORE -o .pio/host/pool_test
    exec .pio/host/pool_test "$@"
fi
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_core.cpp $CORE -o .pio/host/test_core
.pio/host/test_core
python3 test/host/test_sha256_oracle.py
g++ -std=c++17 -O2 -Wall -Isrc test/host/test_v2_core.cpp -o .pio/host/test_v2_core
.pio/host/test_v2_core
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_bm1370.cpp src/core/bm1370.cpp $CORE -o .pio/host/test_bm1370
.pio/host/test_bm1370
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_bm1397.cpp src/core/bm1397.cpp src/core/bm1370.cpp $CORE -o .pio/host/test_bm1397
.pio/host/test_bm1397
g++ -std=c++17 -O2 -Wall -Itest/host/stubs/nerdnos -Itest/host/stubs/miner -Isrc -I"$JSON" test/host/test_nerdnos.cpp src/core/bm1397.cpp src/core/bm1370.cpp $CORE -o .pio/host/test_nerdnos
.pio/host/test_nerdnos
g++ -std=c++17 -O2 -Wall -Itest/host/stubs/gamma -Itest/host/stubs/miner -Isrc test/host/test_gamma601.cpp src/core/bm1370.cpp -o .pio/host/test_gamma601
.pio/host/test_gamma601
g++ -std=c++17 -O2 -Wall -Wno-unused-variable -ffunction-sections -fdata-sections -Itest/host/stubs/miner -Isrc test/host/test_asic_miner.cpp src/core/sha256.cpp src/core/work.cpp -Wl,--gc-sections -o .pio/host/test_asic_miner
.pio/host/test_asic_miner
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_pool_agent.cpp $CORE -o .pio/host/test_pool_agent
.pio/host/test_pool_agent
python3 test/host/test_net_session.py
python3 test/host/test_market_pool.py
python3 test/host/test_clock_fallback.py
python3 test/host/test_tls.py
python3 test/host/test_tls_body.py
python3 test/host/test_share.py
python3 test/host/test_remote.py
python3 test/host/test_mqtt.py
python3 test/host/test_open_dashboard.py
python3 test/host/test_thermal_default.py
python3 test/host/test_restart_ownership.py
python3 test/host/test_trial.py
python3 test/host/test_notify.py
python3 test/host/test_sv2.py
python3 test/host/test_hw_resume.py
python3 test/host/test_hw_overlap_s3.py
g++ -std=c++17 -O2 -Wall -Isrc test/host/test_image_marker.cpp -o .pio/host/test_image_marker
.pio/host/test_image_marker
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_line_buffer.cpp $CORE -o .pio/host/test_line_buffer
.pio/host/test_line_buffer
g++ -std=c++17 -O2 -Wall -Isrc -I"$JSON" test/host/test_web_validation.cpp -o .pio/host/test_web_validation
.pio/host/test_web_validation
g++ -std=c++17 -O2 -Wall -Wno-unused-variable -ffunction-sections -fdata-sections \
    -Itest/host/stubs/miner -Isrc test/host/test_miner_queue.cpp \
    src/core/sha256.cpp src/core/work.cpp -Wl,--gc-sections -o .pio/host/test_miner_queue
.pio/host/test_miner_queue
g++ -std=c++17 -O2 -Wall -Wno-unused-variable -ffunction-sections -fdata-sections \
    -Itest/host/stubs/miner -Isrc test/host/test_block_candidate.cpp \
    src/core/sha256.cpp src/core/work.cpp -Wl,--gc-sections -o .pio/host/test_block_candidate
.pio/host/test_block_candidate
python3 test/host/test_block_alert.py
python3 test/host/test_daily_summary.py
python3 test/host/test_ui_duration.py
g++ -std=c++17 -O2 -Wall -Itest/host/stubs/block_status -Itest/host/stubs/miner -Isrc -I"$JSON" \
    test/host/test_block_status.cpp src/core/work.cpp src/core/sha256.cpp -o .pio/host/test_block_status
.pio/host/test_block_status
python3 test/host/test_block_e2e.py
g++ -std=c++17 -O2 -Wall -pthread -Isrc test/host/test_mining_safety.cpp -o .pio/host/test_mining_safety
.pio/host/test_mining_safety
g++ -std=c++17 -O2 -Wall -Isrc test/host/test_fan_control.cpp -o .pio/host/test_fan_control
.pio/host/test_fan_control
g++ -std=c++17 -O2 -Wall -Itest/host/stubs/miner -Isrc -I"$JSON" \
    test/host/test_settings_apply.cpp src/core/sv2_key.cpp src/core/work.cpp src/core/sha256.cpp -o .pio/host/test_settings_apply
.pio/host/test_settings_apply
g++ -std=c++17 -O2 -Wall -Itest/host/stubs/miner -Isrc -I"$JSON" test/host/test_settings_store.cpp -o .pio/host/test_settings_store
.pio/host/test_settings_store
python3 test/host/test_settings_persistence.py
python3 test/host/test_web_upload.py
python3 test/host/test_web_delivery.py
python3 test/host/test_log_access.py
python3 test/host/test_ghupdate.py
python3 test/host/test_update_redirects.py

# Gamma-only schema: frequency bounds, staged values and old-record migration.
g++ -std=c++17 -O2 -Wall -DBOARD_GAMMA601 -DBOARD_ASIC -Itest/host/stubs/miner -Isrc -I"$JSON" test/host/test_settings_apply.cpp src/core/sv2_key.cpp src/core/work.cpp src/core/sha256.cpp -o .pio/host/test_gamma_settings_apply
.pio/host/test_gamma_settings_apply
g++ -std=c++17 -O2 -Wall -DBOARD_GAMMA601 -DBOARD_ASIC -Itest/host/stubs/miner -Isrc -I"$JSON" test/host/test_settings_store.cpp -o .pio/host/test_gamma_settings_store
.pio/host/test_gamma_settings_store
