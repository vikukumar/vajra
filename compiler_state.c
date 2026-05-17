#include <stdlib.h>
#include <string.h>
#include <stdio.h>

// ----------------------------------------------------
// AST Node Structure
// ----------------------------------------------------
typedef struct ASTNode {
    const char* type;
    const char* name;
    const char* value;
    const char* op;
    struct ASTNode* left;
    struct ASTNode* right;
    struct ASTNode** body;
    long long body_size;
    long long body_cap;
    struct ASTNode** args;
    long long args_size;
    long long args_cap;
    char** params;
    long long params_size;
    long long params_cap;
} ASTNode;

ASTNode* ast_new() {
    ASTNode* node = (ASTNode*)malloc(sizeof(ASTNode));
    memset(node, 0, sizeof(ASTNode));
    return node;
}

void ast_set_type(ASTNode* node, const char* val) { node->type = val; }
const char* ast_get_type(ASTNode* node) { return node ? node->type : ""; }

void ast_set_name(ASTNode* node, const char* val) { node->name = val; }
const char* ast_get_name(ASTNode* node) { return node ? node->name : ""; }

void ast_set_value(ASTNode* node, const char* val) { node->value = val; }
const char* ast_get_value(ASTNode* node) { return node ? node->value : ""; }

void ast_set_op(ASTNode* node, const char* val) { node->op = val; }
const char* ast_get_op(ASTNode* node) { return node ? node->op : ""; }

void ast_set_left(ASTNode* node, ASTNode* val) { node->left = val; }
ASTNode* ast_get_left(ASTNode* node) { return node ? node->left : NULL; }

void ast_set_right(ASTNode* node, ASTNode* val) { node->right = val; }
ASTNode* ast_get_right(ASTNode* node) { return node ? node->right : NULL; }

void ast_push_param(ASTNode* node, const char* param) {
    if (node->params_size >= node->params_cap) {
        node->params_cap = node->params_cap == 0 ? 4 : node->params_cap * 2;
        node->params = (char**)realloc(node->params, sizeof(char*) * node->params_cap);
    }
    node->params[node->params_size++] = (char*)param;
}

void ast_push_body(ASTNode* node, ASTNode* item) {
    if (node->body_size >= node->body_cap) {
        node->body_cap = node->body_cap == 0 ? 4 : node->body_cap * 2;
        node->body = (ASTNode**)realloc(node->body, sizeof(ASTNode*) * node->body_cap);
    }
    node->body[node->body_size++] = item;
}

long long ast_get_body_size(ASTNode* node) { return node ? node->body_size : 0; }
ASTNode* ast_get_body_item(ASTNode* node, long long idx) { return node ? node->body[idx] : NULL; }

void ast_push_arg(ASTNode* node, ASTNode* item) {
    if (node->args_size >= node->args_cap) {
        node->args_cap = node->args_cap == 0 ? 4 : node->args_cap * 2;
        node->args = (ASTNode**)realloc(node->args, sizeof(ASTNode*) * node->args_cap);
    }
    node->args[node->args_size++] = item;
}

long long ast_get_args_size(ASTNode* node) { return node ? node->args_size : 0; }
ASTNode* ast_get_args_item(ASTNode* node, long long idx) { return node ? node->args[idx] : NULL; }


// ----------------------------------------------------
// Lexer & Parser Coordinator State Bridge
// ----------------------------------------------------
extern long long skip_whitespace_and_comments(const char* input, long long pos);
extern long long lex_next_token(const char* input, long long pos, const char* quote_char);
extern const char* substring(const char* s, long long start, long long end);
extern const char* get_quote_char();

typedef struct {
    const char* content;
    long long len;
    long long pos;
    long long cur_kind;
    const char* cur_val;
    long long peek_kind;
    const char* peek_val;
} ParserState;

ParserState* create_parser_state(const char* content) {
    ParserState* s = (ParserState*)malloc(sizeof(ParserState));
    s->content = content;
    s->len = strlen(content);
    s->pos = 0;
    
    printf("create_parser_state: content length = %lld\n", s->len);
    printf("First 40 chars: ");
    for (int i = 0; i < 40 && i < s->len; i++) {
        printf("[%d:%c] ", s->content[i], s->content[i]);
    }
    printf("\n");
    
    // Fill current token
    s->pos = skip_whitespace_and_comments(s->content, s->pos);
    printf("create_parser_state: pos after skip = %lld, len = %lld\n", s->pos, s->len);
    fflush(stdout);
    if (s->pos >= s->len) {
        s->cur_kind = 1;
        s->cur_val = "";
    } else {
        long long packed = lex_next_token(s->content, s->pos, get_quote_char());
        long long kind = packed % 256;
        long long length = packed / 256;
        s->cur_kind = kind;
        s->cur_val = substring(s->content, s->pos, s->pos + length);
        s->pos += length;
        printf("create_parser_state: cur_kind=%lld, cur_val='%s', next_pos=%lld\n", s->cur_kind, s->cur_val, s->pos);
        fflush(stdout);
    }
    
    // Fill peek token
    long long temp_pos = skip_whitespace_and_comments(s->content, s->pos);
    if (temp_pos >= s->len) {
        s->peek_kind = 1;
        s->peek_val = "";
    } else {
        long long packed = lex_next_token(s->content, temp_pos, get_quote_char());
        long long kind = packed % 256;
        long long length = packed / 256;
        s->peek_kind = kind;
        s->peek_val = substring(s->content, temp_pos, temp_pos + length);
        printf("create_parser_state: peek_kind=%lld, peek_val='%s'\n", s->peek_kind, s->peek_val);
        fflush(stdout);
    }
    
    return s;
}

long long parser_get_cur_kind(ParserState* s) { return s->cur_kind; }
const char* parser_get_cur_val(ParserState* s) { return s->cur_val; }
long long parser_get_peek_kind(ParserState* s) { return s->peek_kind; }
const char* parser_get_peek_val(ParserState* s) { return s->peek_val; }

void parser_advance(ParserState* s) {
    s->cur_kind = s->peek_kind;
    s->cur_val = s->peek_val;
    
    // Internal pos has already consumed current token.
    // Fill new peek token.
    long long temp_pos = skip_whitespace_and_comments(s->content, s->pos);
    printf("parser_advance: pos=%lld, temp_pos=%lld, cur_kind=%lld, len=%lld\n", s->pos, temp_pos, s->cur_kind, s->len);
    fflush(stdout);
    if (temp_pos >= s->len) {
        s->peek_kind = 1;
        s->peek_val = "";
    } else {
        long long packed = lex_next_token(s->content, temp_pos, get_quote_char());
        long long kind = packed % 256;
        long long length = packed / 256;
        s->peek_kind = kind;
        s->peek_val = substring(s->content, temp_pos, temp_pos + length);
        s->pos = temp_pos + length; // Advance position for next call
        printf("parser_advance: peek_kind=%lld, peek_val='%s', next_pos=%lld\n", s->peek_kind, s->peek_val, s->pos);
        fflush(stdout);
    }
}


// ----------------------------------------------------
// Code Generator State
// ----------------------------------------------------
typedef struct {
    const char* target_platform;
    char* output_ir;
    long long ir_len;
    long long ir_cap;
    char* global_ir;
    long long glob_len;
    long long glob_cap;
    long long local_var_counter;
    long long label_counter;
} CGState;

CGState* cg_new(const char* target) {
    CGState* s = (CGState*)malloc(sizeof(CGState));
    s->target_platform = target;
    s->ir_cap = 65536;
    s->output_ir = (char*)malloc(s->ir_cap);
    s->output_ir[0] = '\0';
    s->ir_len = 0;
    
    s->glob_cap = 65536;
    s->global_ir = (char*)malloc(s->glob_cap);
    s->global_ir[0] = '\0';
    s->glob_len = 0;
    
    s->local_var_counter = 0;
    s->label_counter = 0;
    return s;
}

const char* cg_get_target_platform(CGState* s) { return s->target_platform; }

void cg_emit(CGState* s, const char* line) {
    long long len = strlen(line) + 6;
    if (s->ir_len + len >= s->ir_cap) {
        s->ir_cap *= 2;
        s->output_ir = (char*)realloc(s->output_ir, s->ir_cap);
    }
    strcat(s->output_ir, "    ");
    strcat(s->output_ir, line);
    strcat(s->output_ir, "\n");
    s->ir_len = strlen(s->output_ir);
}

void cg_emit_global(CGState* s, const char* line) {
    long long len = strlen(line) + 2;
    if (s->glob_len + len >= s->glob_cap) {
        s->glob_cap *= 2;
        s->global_ir = (char*)realloc(s->global_ir, s->glob_cap);
    }
    strcat(s->global_ir, line);
    strcat(s->global_ir, "\n");
    s->glob_len = strlen(s->global_ir);
}

const char* cg_next_var(CGState* s) {
    s->local_var_counter++;
    char* buf = (char*)malloc(32);
    sprintf(buf, "%%t%lld", s->local_var_counter);
    return buf;
}

const char* cg_next_label(CGState* s, const char* prefix) {
    s->label_counter++;
    char* buf = (char*)malloc(64);
    sprintf(buf, "%s_%lld", prefix, s->label_counter);
    return buf;
}

const char* cg_get_ir(CGState* s) {
    char* combined = (char*)malloc(s->glob_len + s->ir_len + 1);
    strcpy(combined, s->global_ir);
    strcat(combined, s->output_ir);
    return combined;
}

const char* read_file_content(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return "";
    fseek(f, 0, SEEK_END);
    long long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc(size + 1);
    long long read_bytes = fread(buf, 1, size, f);
    buf[read_bytes] = '\0';
    fclose(f);
    return buf;
}

void write_file_content(const char* path, const char* content) {
    FILE* f = fopen(path, "wb");
    if (!f) return;
    fwrite(content, 1, strlen(content), f);
    fclose(f);
}

const char* get_quote_char() {
    return "\"";
}

const char* get_newline_char() {
    return "\n";
}

const char* get_tab_char() {
    return "\t";
}

const char* get_cr_char() {
    return "\r";
}
