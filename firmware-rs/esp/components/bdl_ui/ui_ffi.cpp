// C functions over ui_view::View (src/ui_view.h) for the Rust firmware, and
// the M5StickC Plus2 panel: ST7789V2 135x240 on SPI, PWM backlight on GPIO27
// (as M5GFX configures it for that board).
#include "bdl_ui.h"

#include <M5GFX.h>

#include <memory>
#include <new>
#include <string>
#include <vector>

#include "ui_view.h"

namespace {

class StickCPlus2 : public lgfx::LGFX_Device {
    lgfx::Panel_ST7789 panel_;
    lgfx::Bus_SPI bus_;
    lgfx::Light_PWM light_;

public:
    StickCPlus2() {
        {
            auto cfg = bus_.config();
            cfg.spi_host = SPI3_HOST;
            cfg.spi_mode = 0;
            cfg.freq_write = 40000000;
            cfg.freq_read = 16000000;
            cfg.spi_3wire = true;
            cfg.use_lock = true;
            cfg.dma_channel = SPI_DMA_CH_AUTO;
            cfg.pin_sclk = 13;
            cfg.pin_mosi = 15;
            cfg.pin_miso = -1;
            cfg.pin_dc = 14;
            bus_.config(cfg);
            panel_.setBus(&bus_);
        }
        {
            auto cfg = panel_.config();
            cfg.pin_cs = 5;
            cfg.pin_rst = 12;
            cfg.pin_busy = -1;
            cfg.panel_width = 135;
            cfg.panel_height = 240;
            cfg.offset_x = 52;
            cfg.offset_y = 40;
            cfg.offset_rotation = 0;
            cfg.readable = false;
            cfg.invert = true;
            cfg.rgb_order = false;
            cfg.dlen_16bit = false;
            cfg.bus_shared = false;
            panel_.config(cfg);
        }
        {
            auto cfg = light_.config();
            cfg.pin_bl = 27;
            cfg.invert = false;
            cfg.freq = 12000;
            cfg.pwm_channel = 7;
            light_.config(cfg);
            panel_.setLight(&light_);
        }
        setPanel(&panel_);
    }
};

StickCPlus2 *lcd = nullptr;
std::unique_ptr<ui_view::View> view;

std::string str(const char *s) { return s ? s : ""; }

}  // namespace

extern "C" {

bool bdl_ui_begin(uint8_t rotation, uint32_t accent, uint8_t language) {
    if (view) return true;
    lcd = new (std::nothrow) StickCPlus2();
    if (!lcd || !lcd->init()) return false;
    lcd->setRotation(rotation);
    view.reset(new (std::nothrow) ui_view::View(lcd, true, accent, language));
    return view != nullptr;
}

void bdl_ui_brightness(uint8_t percent) {
    if (lcd) lcd->setBrightness(percent > 100 ? 255 : percent * 255 / 100);
}

void bdl_ui_rotation(uint8_t rotation) {
    if (!lcd || lcd->getRotation() == rotation) return;
    lcd->setRotation(rotation);
    if (view) view->invalidate();
}

void bdl_ui_invalidate(void) {
    if (view) view->invalidate();
}

void bdl_ui_render(const bdl_ui_model *m) {
    if (!view || !m) return;
    static std::vector<ui_view::Point> points;
    ui_view::Model model;
    model.page = m->page;
    model.language = m->language;
    model.accent = m->accent;
    model.statusColor = m->status_color;
    model.title = str(m->title);
    model.status = str(m->status);
    model.battery = str(m->battery);
    model.footer = str(m->footer);
    for (int i = 0; i < 6; i++) {
        model.metrics[i].label = str(m->metrics[i].label);
        model.metrics[i].value = str(m->metrics[i].value);
        model.metrics[i].detail = str(m->metrics[i].detail);
        model.metrics[i].color = m->metrics[i].color;
    }
    points.resize(m->point_count > 0 ? m->point_count : 0);
    for (size_t i = 0; i < points.size(); i++) points[i] = {m->points[i].ms, m->points[i].rate};
    model.points = points.data();
    model.pointCount = int(points.size());
    model.graphRevision = m->graph_revision;
    view->render(model);
}

void bdl_ui_message(const char *title, const char *line1, const char *line2) {
    if (view) view->message(title, line1, line2);
}

void bdl_ui_portal(const char *title, const char *ssid, const char *password, const char *qr) {
    if (view) view->portal(title, ssid, password, qr);
}

void bdl_ui_alert(const char *title, const char *value, const char *line) {
    if (view) view->alert(title, value, line);
}

}  // extern "C"
