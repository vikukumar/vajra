.code
myfunc PROC
    mov qword ptr [rbp - 8], rax
    mov rax, qword ptr [rbp - 8]
    ret
myfunc ENDP
END
