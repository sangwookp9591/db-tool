// Bounded reclaim experiment: at most 2 GiB, random pages, 10 seconds alive.
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <dispatch/dispatch.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    char *end;
    unsigned long mib = strtoul(argv[1], &end, 10);
    if (*end || !mib || mib > 2048) return 2;
    dispatch_source_t pressure = dispatch_source_create(DISPATCH_SOURCE_TYPE_MEMORYPRESSURE, 0, DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL, dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0));
    if (!pressure) return 5;
    dispatch_source_set_event_handler(pressure, ^{ printf("memory-pressure-event=%lu\n", dispatch_source_get_data(pressure)); fflush(stdout); });
    dispatch_resume(pressure);
    size_t bytes = mib * 1024UL * 1024UL;
    void *pages = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (pages == MAP_FAILED) return 3;
    arc4random_buf(pages, bytes);
    puts("allocated"); fflush(stdout);
    sleep(10);
    dispatch_source_cancel(pressure);
    return munmap(pages, bytes) == 0 ? 0 : 4;
}
