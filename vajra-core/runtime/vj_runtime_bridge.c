#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

// Direct libc wrapper mappings to bridge Vajra bare-metal binary to windows/libc intrinsics
char* read_file_content(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return "";
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = malloc(size + 1);
    fread(buf, 1, size, f);
    buf[size] = '\0';
    fclose(f);
    return buf;
}

// Global manifest settings holding registers
static char g_manifest_name[128] = "dist/bin/output_stage1.exe";
static char g_manifest_version[32] = "1.0.0";
static char g_manifest_target[32] = "windows";
static char g_manifest_entry[128] = "hello.vj";

static long long g_argc = 0;
static char** g_argv = NULL;

void store_argv_c(long long argc, char** argv) {
    g_argc = argc;
    g_argv = argv;
}

const char* get_argv_item_c(long long idx) {
    if (idx >= g_argc || g_argv == NULL) {
        return "";
    }
    return g_argv[idx];
}

void set_manifest_name(const char* val) { strncpy(g_manifest_name, val, sizeof(g_manifest_name)-1); }
const char* get_manifest_name_val() { return g_manifest_name; }

void set_manifest_version(const char* val) { strncpy(g_manifest_version, val, sizeof(g_manifest_version)-1); }
const char* get_manifest_version_val() { return g_manifest_version; }

void set_manifest_target(const char* val) { strncpy(g_manifest_target, val, sizeof(g_manifest_target)-1); }
const char* get_manifest_target_val() { return g_manifest_target; }

void set_manifest_entry(const char* val) { strncpy(g_manifest_entry, val, sizeof(g_manifest_entry)-1); }
const char* get_manifest_entry_val() { return g_manifest_entry; }

static char g_manifest_description[256] = "Vajra language compiler toolchain";
static char g_manifest_icon[256] = "";

void set_manifest_description(const char* val) { strncpy(g_manifest_description, val, sizeof(g_manifest_description)-1); }
const char* get_manifest_description_val() { return g_manifest_description; }

void set_manifest_icon(const char* val) { strncpy(g_manifest_icon, val, sizeof(g_manifest_icon)-1); }
const char* get_manifest_icon_val() { return g_manifest_icon; }

// Lexer special characters wrappers
const char* get_newline_char() { return "\n"; }
const char* get_tab_char() { return "\t"; }
const char* get_cr_char() { return "\r"; }
const char* get_quote_char() { return "\""; }


// Low-level memory pointer byte evaluations for bare-metal portability
long long pointer_read_i64(long long* ptr, long long offset) {
    return ptr[offset];
}

void pointer_write_i64(long long* ptr, long long offset, long long val) {
    ptr[offset] = val;
}

unsigned char pointer_read_byte(unsigned char* ptr, long long offset) {
    return ptr[offset];
}

void pointer_write_byte(unsigned char* ptr, long long offset, unsigned char val) {
    ptr[offset] = val;
}

// Parallel spawning worker thread pool hook
void invoke_function_pointer(void (*fn)()) {
    if (fn) fn();
}

// Concurrent assertion tracking variables
static long long g_test_total = 0;
static long long g_test_passed = 0;
static long long g_test_failed = 0;

void set_test_total_count(long long val) { g_test_total = val; }
long long get_test_total_count() { return g_test_total; }

void set_test_passed_count(long long val) { g_test_passed = val; }
long long get_test_passed_count() { return g_test_passed; }

void set_test_failed_count(long long val) { g_test_failed = val; }
long long get_test_failed_count() { return g_test_failed; }

// Compiler symbol table global trackers
static char g_symbol_names[1024][64];
static long long g_symbol_size = 0;
static long long g_symbol_cap = 1024;

void set_symbol_table_size(long long val) { g_symbol_size = val; }
long long get_symbol_table_size() { return g_symbol_size; }
long long get_symbol_table_cap() { return g_symbol_cap; }

void set_symbol_table_names(char* names) { /* stub */ }
char* get_symbol_table_names() { return (char*)g_symbol_names; }

// Executable memory caller helper for dynamic JIT engine
void call_executable_memory(void* address) {
    void (*fn)() = (void (*)())address;
    if (fn) fn();
}

int mprotect(void *addr, size_t len, int prot) {
    return 0;
}

const char* read_line_repl(const char* prompt) {
    printf("%s", prompt);
    static char buf[1024];
    if (!fgets(buf, sizeof(buf), stdin)) {
        return "pranam";
    }
    size_t len = strlen(buf);
    if (len > 0 && buf[len - 1] == '\n') {
        buf[len - 1] = '\0';
    }
    return buf;
}

extern void compile_main();
extern void runtime_init(long long argc, char** argv);

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IONBF, 0);
    store_argv_c(argc, argv);
    runtime_init(argc, (char**)argv);
    compile_main();
    return 0;
}
