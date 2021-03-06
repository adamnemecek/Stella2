#pragma once

#include <gtk/gtk.h>
#include <stdint.h>

// Defined in `window.rs`
extern void tcw_wnd_widget_draw_handler(size_t wnd_ptr, cairo_t *cr);
extern void tcw_wnd_widget_dpi_scale_changed_handler(size_t wnd_ptr);
extern int tcw_wnd_widget_nc_hit_test_handler(size_t wnd_ptr, float x, float y);
extern int tcw_wnd_widget_key_press_handler(size_t wnd_ptr, GdkEventKey *event);
extern int tcw_wnd_widget_key_release_handler(size_t wnd_ptr,
                                              GdkEventKey *event);
extern void tcw_wnd_widget_button_handler(size_t wnd_ptr, float x, float y,
                                          int is_pressed, int button);
extern void tcw_wnd_widget_motion_handler(size_t wnd_ptr, float x, float y);
extern void tcw_wnd_widget_leave_handler(size_t wnd_ptr);
extern void tcw_wnd_widget_discrete_scroll_handler(size_t wnd_ptr, float x,
                                                   float y, float delta_x,
                                                   float delta_y);
extern void tcw_wnd_widget_smooth_scroll_handler(size_t wnd_ptr, float x,
                                                 float y, float delta_x,
                                                 float delta_y, uint32_t time);
extern void tcw_wnd_widget_smooth_scroll_stop_handler(size_t wnd_ptr,
                                                      uint32_t time);
