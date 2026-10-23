// SPDX-License-Identifier: GPL-3.0-or-later
// 最小 CLAP 宿主：加载 .clap 并打印插件描述、音频端口、参数等
// 用途：确认「插件到底有没有声明音频输入端口」——这决定了 AU 包装后
// 是一个真效果器，还是被 clap-wrapper 补了"静音占位输入总线"。
#include <clap/clap.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

static const void *host_get_extension(const clap_host_t *h, const char *id) {
    (void)h;
    if (strcmp(id, CLAP_EXT_LOG) == 0 || strcmp(id, CLAP_EXT_THREAD_CHECK) == 0) return NULL;
    return NULL;
}
static void host_request_restart(const clap_host_t *h) { (void)h; }
static void host_request_process(const clap_host_t *h) { (void)h; }
static void host_request_callback(const clap_host_t *h) { (void)h; }

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "用法: %s <plugin.clap>\n", argv[0]); return 1; }
    // .clap 是目录包：真正的 dylib 在 Contents/MacOS/<name>
    const char *base = strrchr(argv[1], '/');
    base = base ? base + 1 : argv[1];
    char name[512];
    snprintf(name, sizeof(name), "%s", base);
    char *dot = strstr(name, ".clap");
    if (dot) *dot = '\0';
    char path[4096];
    snprintf(path, sizeof(path), "%s/Contents/MacOS/%s", argv[1], name);

    void *h = dlopen(path, RTLD_LAZY | RTLD_LOCAL);
    if (!h) { printf("dlopen 失败: %s\n", dlerror()); return 2; }
    const clap_plugin_entry_t *entry = (const clap_plugin_entry_t *)dlsym(h, "clap_entry");
    if (!entry) { printf("找不到 clap_entry\n"); return 3; }
    if (!entry->init(argv[1])) { printf("entry->init 失败\n"); return 4; }

    const clap_plugin_factory_t *fac =
        (const clap_plugin_factory_t *)entry->get_factory(CLAP_PLUGIN_FACTORY_ID);
    if (!fac) { printf("没有 plugin factory\n"); return 5; }

    clap_host_t host = {0};
    host.clap_version = CLAP_VERSION;
    host.name = "probe";
    host.vendor = "rustomp";
    host.url = "";
    host.version = "0";
    host.get_extension = host_get_extension;
    host.request_restart = host_request_restart;
    host.request_process = host_request_process;
    host.request_callback = host_request_callback;

    uint32_t n = fac->get_plugin_count(fac);
    printf("插件数: %u\n", n);
    for (uint32_t i = 0; i < n; i++) {
        const clap_plugin_descriptor_t *d = fac->get_plugin_descriptor(fac, i);
        printf("\n[%u] id=%s\n    name=%s  vendor=%s  version=%s\n", i, d->id, d->name, d->vendor, d->version);
        printf("    features:");
        for (const char **f = d->features; f && *f; f++) printf(" %s", *f);
        printf("\n");
        const clap_plugin_t *p = fac->create_plugin(fac, &host, d->id);
        if (!p) { printf("    create_plugin 失败\n"); continue; }
        p->init(p);
        const clap_plugin_audio_ports_t *ap =
            (const clap_plugin_audio_ports_t *)p->get_extension(p, CLAP_EXT_AUDIO_PORTS);
        if (!ap) { printf("    没有 audio-ports 扩展\n"); }
        else {
            for (int in = 0; in <= 1; in++) {
                uint32_t cnt = ap->count(p, in != 0);
                printf("    %s 端口数: %u\n", in ? "输入" : "输出", cnt);
                for (uint32_t j = 0; j < cnt; j++) {
                    clap_audio_port_info_t info;
                    if (ap->get(p, j, in != 0, &info))
                        printf("      [%u] %s  channels=%u  type=%s\n", j, info.name,
                               info.channel_count, info.port_type ? info.port_type : "?");
                }
            }
        }
        const clap_plugin_params_t *pp =
            (const clap_plugin_params_t *)p->get_extension(p, CLAP_EXT_PARAMS);
        if (pp) printf("    参数数: %u\n", pp->count(p));
        p->destroy(p);
    }
    entry->deinit();
    return 0;
}
