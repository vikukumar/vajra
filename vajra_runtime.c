#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#endif

// Global argc and argv for Vajra orchestrator
long long argc;
char** argv;

void* vajra_gc_alloc(size_t size) {
    return malloc(size);
}

void vajra_gc_free(void* ptr) {
    free(ptr);
}

const char* get_char(const char* s, long long i) {
    char* res = (char*)malloc(2);
    res[0] = s[i];
    res[1] = '\0';
    return res;
}

const char* substring(const char* s, long long start, long long end) {
    long long len = end - start;
    if (len <= 0) {
        char* res = (char*)malloc(1);
        res[0] = '\0';
        return res;
    }
    char* res = (char*)malloc(len + 1);
    for (long long idx = 0; idx < len; idx++) {
        res[idx] = s[start + idx];
    }
    res[len] = '\0';
    return res;
}

const char* concat2(const char* s1, const char* s2) {
    size_t l1 = s1 ? strlen(s1) : 0;
    size_t l2 = s2 ? strlen(s2) : 0;
    char* r = (char*)malloc(l1 + l2 + 1);
    if (s1) strcpy(r, s1); else r[0] = '\0';
    if (s2) strcat(r, s2);
    return r;
}

const char* concat3(const char* s1, const char* s2, const char* s3) {
    size_t l1 = s1 ? strlen(s1) : 0;
    size_t l2 = s2 ? strlen(s2) : 0;
    size_t l3 = s3 ? strlen(s3) : 0;
    char* r = (char*)malloc(l1 + l2 + l3 + 1);
    if (s1) strcpy(r, s1); else r[0] = '\0';
    if (s2) strcat(r, s2);
    if (s3) strcat(r, s3);
    return r;
}

const char* concat4(const char* s1, const char* s2, const char* s3, const char* s4) {
    size_t l1 = s1 ? strlen(s1) : 0;
    size_t l2 = s2 ? strlen(s2) : 0;
    size_t l3 = s3 ? strlen(s3) : 0;
    size_t l4 = s4 ? strlen(s4) : 0;
    char* r = (char*)malloc(l1 + l2 + l3 + l4 + 1);
    if (s1) strcpy(r, s1); else r[0] = '\0';
    if (s2) strcat(r, s2);
    if (s3) strcat(r, s3);
    if (s4) strcat(r, s4);
    return r;
}

const char* concat5(const char* s1, const char* s2, const char* s3, const char* s4, const char* s5) {
    size_t l1 = s1 ? strlen(s1) : 0;
    size_t l2 = s2 ? strlen(s2) : 0;
    size_t l3 = s3 ? strlen(s3) : 0;
    size_t l4 = s4 ? strlen(s4) : 0;
    size_t l5 = s5 ? strlen(s5) : 0;
    char* r = (char*)malloc(l1 + l2 + l3 + l4 + l5 + 1);
    if (s1) strcpy(r, s1); else r[0] = '\0';
    if (s2) strcat(r, s2);
    if (s3) strcat(r, s3);
    if (s4) strcat(r, s4);
    if (s5) strcat(r, s5);
    return r;
}

void print_i64(long long val) {
    printf("%lld\n", val);
}

void print_string(const char* s) {
    printf("%s\n", s);
}

const char* get_argv_item(char** argv, long long idx) {
    return argv[idx];
}

void vajra_throw_exception(const char* msg) {
    fprintf(stderr, "क्रैश! अनपेक्षित अपवाद (Unhandled Exception): %s\n", msg);
    exit(1);
}

void __main(void) {
#ifdef _WIN32
    SetConsoleOutputCP(65001);
#endif
}
// ----------------------------------------------------
// Multithreading Support
// ----------------------------------------------------
typedef struct {
    void (*worker_fn)(long long, long long, long long*, long long);
    long long start;
    long long end;
    long long* sum_ptr;
    long long nodes;
} ParallelTask;

#ifdef _WIN32
DWORD WINAPI parallel_thread_proc_helper(LPVOID param) {
    ParallelTask* task = (ParallelTask*)param;
    task->worker_fn(task->start, task->end, task->sum_ptr, task->nodes);
    return 0;
}

DWORD WINAPI spawn_thread_proc_helper(LPVOID param) {
    void (*fn)(void) = (void (*)(void))param;
    fn();
    return 0;
}
#endif

void vajra_spawn(void (*fn)(void)) {
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, spawn_thread_proc_helper, (LPVOID)fn, 0, NULL);
    if (thread != NULL) {
        CloseHandle(thread);
    }
#else
    fn();
#endif
}

void vajra_parallel_for(
    long long start, 
    long long end, 
    void (*worker_fn)(long long, long long, long long*, long long), 
    long long* sum_ptr, 
    long long nodes
) {
#ifdef _WIN32
    SYSTEM_INFO sysinfo;
    GetSystemInfo(&sysinfo);
    int num_threads = sysinfo.dwNumberOfProcessors;
    if (num_threads < 1) num_threads = 1;
    if (num_threads > 64) num_threads = 64;

    long long total_iters = end - start;
    if (total_iters <= 0) return;

    if (total_iters < 100) {
        worker_fn(start, end, sum_ptr, nodes);
        return;
    }

    if (num_threads > total_iters) {
        num_threads = (int)total_iters;
    }

    HANDLE* threads = (HANDLE*)malloc(sizeof(HANDLE) * num_threads);
    ParallelTask* tasks = (ParallelTask*)malloc(sizeof(ParallelTask) * num_threads);

    long long chunk_size = total_iters / num_threads;
    long long rem = total_iters % num_threads;

    long long current_start = start;
    for (int i = 0; i < num_threads; i++) {
        long long current_end = current_start + chunk_size + (i < rem ? 1 : 0);
        
        tasks[i].worker_fn = worker_fn;
        tasks[i].start = current_start;
        tasks[i].end = current_end;
        tasks[i].sum_ptr = sum_ptr;
        tasks[i].nodes = nodes;

        threads[i] = CreateThread(NULL, 0, parallel_thread_proc_helper, &tasks[i], 0, NULL);
        current_start = current_end;
    }

    WaitForMultipleObjects(num_threads, threads, TRUE, INFINITE);

    for (int i = 0; i < num_threads; i++) {
        CloseHandle(threads[i]);
    }

    free(threads);
    free(tasks);
#else
    worker_fn(start, end, sum_ptr, nodes);
#endif
}
