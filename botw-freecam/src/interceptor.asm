.data
EXTERN g_get_camera_data: qword
EXTERN g_camera_active: byte
EXTERN g_camera_struct: qword

EXTERN dummy_xinput: qword
EXTERN g_xinput_override: qword

.code
asm_get_camera_data PROC
    pushf

    ; Steal the camera pointer
    push rbx
    lea rbx, [r13 + rdx + 654h]
    sub rbx, 24h
    mov [g_camera_struct], rbx
    pop rbx

    cmp g_camera_active, 0
    je original
    jmp ending

    original:
    movbe [r13 + rdx + 654h], r14d
    cvtss2sd xmm0, xmm0

    ending:
    popf
    jmp [g_get_camera_data]
asm_get_camera_data ENDP

; HACK: We use an intermediary to replace the function pointer in rax since we
; still don't write a function trampoline because life
asm_override_xinput_call PROC
    ; original code
    ; mov rax, [rax+28]
    ; Instead, we'll move our function pointer to rax to call that one.
    lea rax, dummy_xinput
    lea rdx, [rbp-19h]
    mov ecx, [rdi+00000150h]
    jmp [g_xinput_override]

asm_override_xinput_call ENDP

END
