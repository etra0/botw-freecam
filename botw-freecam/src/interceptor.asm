.data
EXTERN g_get_camera_data: qword
EXTERN g_camera_active: byte
EXTERN g_camera_struct: qword

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


END
