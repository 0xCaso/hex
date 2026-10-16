#ifndef HEX_H
#define HEX_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct hex_engine_s hex_engine_s;
typedef struct hex_engine_s* hex_engine_t;

typedef struct hex_slice_s {
    const char* ptr;
    size_t len;
} hex_slice_s;

typedef enum hex_hud_phase_e {
    HEX_HUD_PHASE_HIDDEN = 0,
    HEX_HUD_PHASE_LISTENING = 1,
    HEX_HUD_PHASE_LOCKED = 2,
    HEX_HUD_PHASE_TRANSCRIBING = 3,
    HEX_HUD_PHASE_VOICE_ACTION = 4,
    HEX_HUD_PHASE_COMPLETED = 5,
    HEX_HUD_PHASE_CANCELLED = 6,
    HEX_HUD_PHASE_ERROR = 7,
} hex_hud_phase_e;

typedef struct hex_hud_telemetry_s {
    hex_hud_phase_e phase;
    float rms_level;
    float peak_level;
    uint32_t active_queue_count;
    uint32_t mode_color_rgba;
    uint64_t phase_started_ns;
    bool is_command_mode_awake;
    bool is_voice_action;
    bool is_post_processing;
    bool is_capturing;
} hex_hud_telemetry_s;

typedef enum hex_event_tag_e {
    HEX_EVENT_READINESS_CHANGED = 1,
    HEX_EVENT_SETTINGS_CHANGED = 2,
    HEX_EVENT_MODEL_PREP_UPDATED = 3,
    HEX_EVENT_SHORTCUT_CAPTURE_UPDATED = 4,
    HEX_EVENT_HISTORY_UPDATED = 5,
    HEX_EVENT_ACTIVITY_APPENDED = 6,
    HEX_EVENT_MEETING_STATE_CHANGED = 7,
    HEX_EVENT_MEETING_TRANSCRIPT_DELTA = 8,
    HEX_EVENT_OPENCODE_CATALOG_UPDATED = 9,
    HEX_EVENT_PERSONAL_COMMANDS_UPDATED = 10,
    HEX_EVENT_DEV_COMMAND_REQUESTED = 11,
    HEX_EVENT_REQUEST_CHECK_FOR_UPDATES = 12,
    HEX_EVENT_HUD_CHANGED = 13,
    HEX_EVENT_MEETING_PANEL_CHANGED = 14,
    HEX_EVENT_QUIT_REQUESTED = 15,
} hex_event_tag_e;

typedef struct hex_event_s {
    hex_event_tag_e tag;
    uint64_t generation;
    hex_slice_s json_payload;
} hex_event_s;

typedef void (*hex_wakeup_cb)(void* userdata);
typedef void (*hex_event_cb)(void* userdata, const hex_event_s* event);

typedef struct hex_runtime_config_s {
    void* userdata;
    hex_wakeup_cb wakeup_cb;
    hex_event_cb event_cb;
    bool developer_features_enabled;
    bool start_workers;
    bool preview_dictation_hud;
} hex_runtime_config_s;

bool hex_keyboard_initialize_layout_on_main_thread(void);
int32_t hex_cli_maybe_run_from_process_args(void);

hex_engine_t hex_engine_new(const hex_runtime_config_s* runtime_cfg);
void hex_engine_free(hex_engine_t engine);
void hex_engine_tick(hex_engine_t engine);

hex_hud_telemetry_s hex_engine_read_hud_telemetry(hex_engine_t engine);

hex_slice_s hex_engine_copy_bootstrap_snapshot_json(hex_engine_t engine);
hex_slice_s hex_engine_copy_history_page_json(hex_engine_t engine, hex_slice_s query_utf8, uint32_t limit);
hex_slice_s hex_engine_copy_meeting_detail_json(hex_engine_t engine, hex_slice_s meeting_id_utf8);
void hex_string_free(hex_slice_s str);

bool hex_engine_dispatch_json(hex_engine_t engine, hex_slice_s action_json, hex_slice_s* out_error_json);

#ifdef __cplusplus
}
#endif

#endif /* HEX_H */
