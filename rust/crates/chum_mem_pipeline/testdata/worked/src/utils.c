#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* WHY: We avoid malloc in hot paths — these helpers use stack buffers
   and only fall back to heap for oversized inputs. */

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buffer;

Buffer buffer_new(size_t initial_cap) {
    Buffer buf;
    buf.data = (char *)malloc(initial_cap);
    buf.len = 0;
    buf.cap = initial_cap;
    return buf;
}

/* NOTE: Returns -1 if realloc fails; caller must check. */
int buffer_append(Buffer *buf, const char *src, size_t n) {
    if (buf->len + n > buf->cap) {
        size_t new_cap = (buf->cap * 2 > buf->len + n) ? buf->cap * 2 : buf->len + n;
        char *tmp = (char *)realloc(buf->data, new_cap);
        if (!tmp) return -1;
        buf->data = tmp;
        buf->cap = new_cap;
    }
    memcpy(buf->data + buf->len, src, n);
    buf->len += n;
    return 0;
}

void buffer_free(Buffer *buf) {
    free(buf->data);
    buf->data = NULL;
    buf->len = buf->cap = 0;
}

int main(void) {
    Buffer b = buffer_new(64);
    buffer_append(&b, "hello", 5);
    printf("len=%zu cap=%zu\n", b.len, b.cap);
    buffer_free(&b);
    return 0;
}
