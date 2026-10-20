// The externs behind Scala Native's `EmbeddedResourceReader`, over the resources embedded in
// `scivolo_aot_resources`.
#include <stdint.h>

typedef struct {
    const char *path;
    int64_t path_length;
    const char *content;
    int64_t content_length;
} Resource;

extern const Resource scivolo_aot_resources[];
extern const int64_t scivolo_aot_resources_count;

static const Resource *resource(int32_t id) {
    return id >= 0 && id < scivolo_aot_resources_count ? &scivolo_aot_resources[id] : 0;
}

int32_t scalanative_resource_get_embedded_size(void) { return (int32_t)scivolo_aot_resources_count; }
int32_t scalanative_resource_get_path_length(int32_t id) {
    return resource(id) ? (int32_t)resource(id)->path_length : 0;
}
const char *scalanative_resource_get_path_ptr(int32_t id) { return resource(id) ? resource(id)->path : 0; }
int32_t scalanative_resource_get_content_length(int32_t id) {
    return resource(id) ? (int32_t)resource(id)->content_length : 0;
}
const char *scalanative_resource_get_content_ptr(int32_t id) {
    return resource(id) ? resource(id)->content : 0;
}
