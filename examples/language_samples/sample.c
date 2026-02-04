/* C sample file for syntax highlighting test */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_SIZE 100
#define SQUARE(x) ((x) * (x))

typedef struct {
    int x;
    int y;
} Point;

typedef enum {
    STATUS_OK = 0,
    STATUS_ERROR = -1,
    STATUS_NOT_FOUND = -2
} Status;

/* Function prototypes */
Point create_point(int x, int y);
double distance(Point a, Point b);
int* create_array(size_t size);
void free_array(int* arr);

/* Create a new Point */
Point create_point(int x, int y) {
    Point p;
    p.x = x;
    p.y = y;
    return p;
}

/* Calculate Euclidean distance between two points */
double distance(Point a, Point b) {
    int dx = a.x - b.x;
    int dy = a.y - b.y;
    return sqrt((double)(dx * dx + dy * dy));
}

/* Dynamic array allocation */
int* create_array(size_t size) {
    int* arr = (int*)malloc(size * sizeof(int));
    if (arr == NULL) {
        fprintf(stderr, "Memory allocation failed\n");
        return NULL;
    }

    for (size_t i = 0; i < size; i++) {
        arr[i] = (int)i * 2;
    }

    return arr;
}

void free_array(int* arr) {
    if (arr != NULL) {
        free(arr);
    }
}

/* Linked list node */
struct Node {
    int data;
    struct Node* next;
};

struct Node* list_push(struct Node* head, int value) {
    struct Node* new_node = (struct Node*)malloc(sizeof(struct Node));
    if (new_node == NULL) {
        return head;
    }
    new_node->data = value;
    new_node->next = head;
    return new_node;
}

void list_print(struct Node* head) {
    struct Node* current = head;
    while (current != NULL) {
        printf("%d -> ", current->data);
        current = current->next;
    }
    printf("NULL\n");
}

int main(int argc, char* argv[]) {
    /* Point operations */
    Point p1 = create_point(0, 0);
    Point p2 = create_point(3, 4);

    printf("Point 1: (%d, %d)\n", p1.x, p1.y);
    printf("Point 2: (%d, %d)\n", p2.x, p2.y);
    printf("Distance: %.2f\n", distance(p1, p2));

    /* Array operations */
    int* numbers = create_array(10);
    if (numbers != NULL) {
        printf("Array: ");
        for (int i = 0; i < 10; i++) {
            printf("%d ", numbers[i]);
        }
        printf("\n");
        free_array(numbers);
    }

    /* Macro usage */
    int value = 5;
    printf("Square of %d is %d\n", value, SQUARE(value));

    /* Switch statement */
    Status status = STATUS_OK;
    switch (status) {
        case STATUS_OK:
            printf("Status: OK\n");
            break;
        case STATUS_ERROR:
            printf("Status: Error\n");
            break;
        default:
            printf("Status: Unknown\n");
            break;
    }

    return 0;
}
