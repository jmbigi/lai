# Inferencia local en CPU para programación autónoma en equipos de baja gama

> **Aviso de honestidad (Regla 6 y Regla 11).**
> Este documento recoge **investigación externa** y órdenes de magnitud
> teóricos. **Ninguna de las cifras citadas ha sido reproducida por lai en este
> repositorio.** No son resultados propios, no son un benchmark de lai y no
> deben citarse como tales. Cuando una cifra aparece, se indica su origen
> (paper, benchmark de terceros o cálculo aritmético de orden de magnitud).
> Antes de adoptar cualquier recomendación, **mídala en su hardware** siguiendo
> el protocolo de la sección 8.

Este documento complementa `BENCHMARKS.md` (resultados propios, medidos) y
`DOCUMENTACION_REGLAS.md` (Regla 11: medir antes de recomendar).

---

## 1. El muro es el ancho de banda, no la potencia de cálculo

La decodificación autorregresiva es una operación de **baja intensidad
aritmética**: por cada token generado se leen muchos bytes de pesos y se hacen
relativamente pocas operaciones. El modelo Roofline formaliza el límite: si la
intensidad aritmética (FLOPs/byte) está por debajo del punto de cresta del
hardware, el rendimiento lo fija el ancho de banda, no la CPU.

Cálculo de orden de magnitud (aritmética, no medición):

| Modelo | Bytes por token (aprox.) | Bus 128 GB/s → techo teórico |
|--------|--------------------------|------------------------------|
| 8B en 8 bits | ~8 GB | ~16 tok/s |
| 3–4B en 4 bits | ~2–2.5 GB | ~50–64 tok/s |

El techo real es bastante menor: hay que descontar caché KV, el SO y las
herramientas de verificación (`pytest`, `rustc`) que compiten por la misma RAM.

**Consecuencia metodológica:** una optimización que no reduzca los bytes leídos
por token —o que no convierta pasos *memory-bound* en *compute-bound* mediante
batching— no cambia el régimen. Esto explica qué técnicas funcionan y cuáles no.

---

## 2. Cuantización: el diablo está en el codebook

### 2.1. Q4_K_M no es automáticamente el óptimo para código

Un benchmark de terceros sobre SWE-bench Verified con un modelo de ~2B reportó
`Q5_K_M` con **62/500** instancias resueltas frente a **27/500** de `Q4_K_M`, y
`Q6_K` con 58/500. Es un factor >2 en tareas reales de ingeniería de software.

*Interpretación:* el código es sensible a la precisión de las representaciones
internas; pequeños errores se amplifican al generar identificadores, firmas y
estructuras de control. El «sweet spot» de 4 bits es un sweet spot de **chat**,
no necesariamente de **código**.

*Implicación:* con 16 GB de RAM, `Q5_K_M` sobre 3–4B puede rendir más que
`Q4_K_M` sobre un modelo apenas mayor. Con 8 GB, `Q4_K_M`/`IQ4_XS` siguen siendo
la opción sensata.

### 2.2. La geometría del codebook importa tanto como el bitrate

Los cuantizadores `IQ*` usan reconstrucción no lineal con tablas de búsqueda y
suelen retener algo más de calidad a igual bitrate, a costa de decodificación
más lenta. Pero la ventaja depende del modelo y del dominio: en pruebas de
terceros con Qwen2.5-Coder-14B, `IQ4_XS` quedó en 84.76% frente a 85.37% de
`Q4_K_M` (diferencia mínima), mientras que en 3 bits `IQ3_XS` superó a `Q3_K_L`
por más de 4 puntos con menos bits.

**No existe un «mejor cuantizador» universal.** La elección óptima depende de la
distribución de pesos del modelo, el dominio y la CPU (AVX-512, AMX, etc.).

### 2.3. Cuantización mixta y por capas

La cuantización uniforme es una simplificación. Mantener en INT8 las capas más
sensibles (típicamente las finales del MLP y las proyecciones de atención) y el
resto en INT4 puede superar a una cuantización uniforme del mismo presupuesto de
bits. La formulación correcta del problema es: **asignar bits por capa para
maximizar pass@1 bajo una restricción de tamaño total**.

---

## 3. Decodificación especulativa en CPU

La idea: un borrador propone varios tokens y el modelo objetivo los verifica en
un pase. Convierte pasos *memory-bound* de un token en operaciones batched.

### 3.1. El cuello de botella es el runtime, no el kernel

Un análisis de sistemas de terceros sobre decodificación especulativa en CPU
atribuye solo el **6.0%** de la ganancia a un kernel ternario AVX2 optimizado,
mientras que la gestión de contexto/caché aportó el **35.0%**. Los costes
dominantes son la reconstrucción de grafos de contexto duales (borrador y
objetivo) y la sobrecarga de *rollback* de la caché KV al rechazar tokens.

**Implicación:** en CPU real, la orquestación de memoria y el cambio de contexto
importan más que el algoritmo de especulación.

### 3.2. El borrador óptimo no siempre es un modelo pequeño

- En sistemas *memory-bound*, un borrador pequeño y rápido ayuda.
- En sistemas *compute-bound* (algunas CPU con iGPU), la especulación puede
  **degradar** el rendimiento porque el coste de propuesta supera el ahorro.

**Alternativa sin borrador: búsqueda de n-gramas (prompt lookup).** Se busca en
el contexto previo la ocurrencia más reciente del sufijo actual y se proponen
los tokens que la siguieron. No requiere modelo auxiliar ni cómputo de
propuesta, y funciona bien en código por la repetición de imports, firmas y
estructuras de control. Para autocompletado de código puede ser más rentable
que un borrador neuronal.

### 3.3. La especulación debe ser degradable

Esquemas de profundidad fija fallan bajo carga variable. Los controladores
adaptativos monitorizan señales del sistema y reducen la profundidad del
borrador (o la desactivan) cuando hay presión de recursos.

**Regla de diseño para un agente:** la especulación es un componente
**degradable**, no un modo fijo. Si `pytest` está consumiendo RAM y CPU, se baja
la profundidad o se apaga; no se fuerza throughput a costa de la estabilidad.

---

## 4. Caché KV: el coste que crece con el contexto

La caché KV no es solo un consumidor pasivo de RAM: **cada paso de atención la
lee**, así que también afecta a la latencia.

### 4.1. Compresión y evicción selectiva

Cuantizar la KV a `q8_0` reduce la huella, pero para contextos largos la
literatura propone más: evicción selectiva (H2O, SnapKV, StreamingLLM) que
descarta tokens con bajo peso atencional acumulado y conserva los *heavy
hitters* más una ventana reciente. `SnapKV` comprime una sola vez durante el
prefill observando la atención de la ventana final, lo que encaja con contextos
largos.

**Implicación para un agente de código:** no todo el repositorio merece estar en
la KV. Un enfoque jerárquico —firmas de funciones, imports y diff activo en
contexto; el resto recuperado con `grep`/índice AST— es más eficiente que
mantener 32K tokens de detalle.

### 4.2. Offloading indexado

En sistemas híbridos, descargar la KV a memoria de CPU y recuperar solo un
subconjunto disperso de tokens ha mostrado ganancias grandes de throughput a
contextos muy largos (RetroInfer reporta hasta **4.4×** a 120K preservando la
precisión de atención completa). En un equipo puramente CPU la lección
transferible es que la KV debería ser una **estructura indexada**, no un array
lineal, y que hay que medir el coste de recuperación frente al ahorro de ancho
de banda.

---

## 5. Modelos ternarios: el mito del 27B y la realidad del 2B

Los ternarios de 27B siguen siendo inviables no por el tamaño de los pesos
(~6 GB) sino por el **volumen de activaciones** y el ancho de banda para
moverlas; la ventaja de los pesos ternarios se diluye cuando las activaciones
dominan el tráfico.

En cambio, **BitNet b1.58 en ~2B** (pesos ternarios −1/0/+1, ~0.4 GB) reporta
5–7 tok/s en una sola CPU y ~29 ms/token de latencia de decodificación. La clave
es que está **entrenado nativamente** en esa precisión, no cuantizado después.

**¿Sirve para programación autónoma?** A 5–7 tok/s no es interactivo para chat,
pero puede bastar en un **bucle de verificación**: si el modelo solo propone
parches pequeños y el compilador/linter/tests absorben el trabajo cognitivo, la
velocidad importa menos que la tasa de acierto al primer intento. Evidencia de
terceros con modelos sub-1B en bucles verificados respalda esta dirección.

---

## 6. MoE: más parámetros no siempre es más lentitud, pero casi siempre es más RAM

Un MoE activa solo algunos expertos por token, lo que reduce FLOPs. El problema
en CPU no es el cómputo: **todos los expertos deben residir en algún sitio** y
el router decide cuáles cargar. Si no caben en RAM, el *page cache* del SO pasa
a ser el gestor de memoria real y la paginación desde SSD destruye la
interactividad.

Conclusiones:

- Un MoE de 30B/80B en 8–16 GB no es «un modelo grande lento»: es un sistema de
  paginación con un modelo adjunto, gobernado por el SSD.
- Un MoE **pequeño** (3–8B totales, 1–2B activos) puede ser competitivo en
  velocidad con un denso similar **si los expertos caben holgadamente en RAM**.

---

## 7. El bucle de verificación como multiplicador de capacidad

El verificador no es solo un filtro: puede **aumentar la capacidad efectiva** del
modelo.

- Un modelo de **0.5B** en un bucle determinista verificar → proponer → aplicar →
  reverificar, con el compilador como oráculo, reportó en trabajo de terceros
  tasas de reparación muy superiores a las de su modelo maestro en distribución
  (86.7% con SFT, 0% sin entrenamiento en el bucle), y mejoras fuera de
  distribución con un currículo basado en fallos.
- RLVR (recompensas verificables con tests y análisis estático) mejora pass@1 en
  modelos de 0.6–1B; las recompensas puramente estáticas pueden sesgar hacia
  completaciones más cortas.

**Implicación:** el verificador es también una **fuente de datos de
entrenamiento**. Un agente que ejecuta `pytest` y `mypy` en cada iteración no
solo rechaza parches: genera señal aprovechable para ajustar modelos pequeños.

---

## 8. Metodología: medir, no adivinar

### 8.1. Calcular el techo teórico antes de optimizar

```
tamaño del modelo + caché KV + reserva del SO + pico de herramientas < RAM libre
```

Si el modelo no cabe, ninguna optimización de software lo hará caber. Si cabe
pero el throughput medido está muy por debajo de
`ancho de banda efectivo / bytes por token`, el cuello de botella es el runtime
(hilos, reconstrucción de grafos, paginación), no la RAM.

### 8.2. Cambiar una sola variable a la vez, con controles intercalados

Registrar por corrida: modelo, cuantización, contexto, KV, hilos, RAM libre,
tokens/s, tiempo por tarea y tasa de éxito. Repetir el control entre variantes
para detectar deriva térmica o de carga.

### 8.3. Compuerta de estabilidad de contexto completo

Una optimización que mejora prompts cortos pero degrada la recuperación en
contextos largos **es una regresión disfrazada**. Antes de adoptar un cambio en
KV, cuantización o especulación, verificar que el modelo sigue localizando y
modificando correctamente un símbolo definido miles de tokens atrás.

### 8.4. NUMA y `mlock` selectivo

- En equipos multiprocesador, la afinidad hilos↔bancos de memoria importa;
  fragmentar pesos entre sockets genera tráfico de coherencia. En un escritorio
  típico es irrelevante; en un servidor reutilizado puede costar más que cambiar
  de cuantización.
- `mlock` indiscriminado puede ser contraproducente: impide que el SO reclame
  páginas no usadas. En MoE cargados con `mmap`, lo razonable es fijar solo los
  expertos más usados y dejar el resto al *page cache*. En densos, mapear con
  `mmap` y fijar únicamente las regiones críticas.

---

## 9. Veredictos sobre afirmaciones habituales

| Afirmación | Veredicto |
|---|---|
| «3–4B en Q4_K_M es el techo práctico» | **Matizable.** Es el techo para 8 GB; con 16 GB, `Q5_K_M` sobre 3–4B puede ser superior para código. |
| «Los MoE de 30B/80B son inviables» | **Cierto en 8–16 GB**, pero un MoE pequeño (3–8B totales) puede ser competitivo si cabe en RAM. |
| «Los ternarios de 27B son inviables» | **Cierto por activaciones**, pero BitNet ~2B (~0.4 GB) es una alternativa real para bucles de verificación. |
| «La decodificación especulativa siempre acelera» | **Solo si el runtime elimina la sobrecarga de contexto y rollback**; si no, puede degradar. |
| «`q8_0` en KV es suficiente» | **Insuficiente para contextos largos**; evicción selectiva y offloading indexado ofrecen más. |
| «El bucle de verificación solo absorbe errores» | **También puede amplificar la capacidad**: un modelo muy pequeño verificado puede superar a su maestro en reparación. |
| «Más hilos que núcleos físicos siempre degrada» | **Cierto en decodificación *memory-bound***; en prefill *compute-bound* puede ayudar hasta el límite de núcleos físicos. |

---

## 10. Recomendación operativa para 8–16 GB sin GPU

1. **Modelo principal:** denso de 3–4B. `Q5_K_M` si hay 16 GB; `Q4_K_M`/`IQ4_XS`
   si hay 8 GB. **Medir pass@1** en una muestra representativa antes de fijar la
   elección.
2. **Caché KV:** `q8_0` como línea base; evaluar evicción selectiva si el
   contexto supera ~8K tokens. No asumir que `q8_0` basta para contextos largos.
3. **Decodificación especulativa:** empezar por **prompt lookup por n-gramas**
   (sin coste de borrador); si se usa borrador neuronal, hacerlo **adaptativo y
   degradable** bajo presión de recursos.
4. **Verificación:** puerta determinista con `ruff` + `mypy` + `pytest` (o
   `cargo check` + `cargo test` en Rust). El verificador no es opcional: define
   la capacidad efectiva del sistema.
5. **Metodología:** calcular el techo, cambiar una variable a la vez, medir con
   controles intercalados y aplicar la compuerta de contexto completo.

**Conclusión:** el límite no es solo el tamaño del modelo, sino la interacción
entre **ancho de banda**, **gestión de memoria del runtime** y **densidad de la
señal de verificación**. Un 0.5B en un bucle verificado puede ser más útil que
un 7B sin verificación; un 3B en `Q5_K_M` puede superar a un 4B en `Q4_K_M` en
código; y un 2B ternario a ~6 tok/s puede bastar si la tarea está bien
descompuesta.

---

## 11. Cómo aplicar esto a lai (sin inventar resultados)

- `lai` ya expone el bucle de verificación (`cargo check`, `cargo test`,
  `pytest` en `repair_engine`/`verification_engine`) y herramientas de lectura
  eficiente (`grep` con regex/glob, `read_file` por rangos) para no gastar
  contexto.
- El contexto del proyecto inyectado en `auto`/`chat` está acotado a 8 000
  caracteres para no desplazar la tarea.
- Para SWE-bench, la guía es `OLLAMA_CONTEXT_LENGTH=16384` y lectura por
  `grep` + rangos. **El efecto real de esa variable sigue pendiente de medición**
  (ver `swebench/README.md`).
- Cualquier cifra que se publique en `BENCHMARKS.md` debe venir de una corrida
  propia, con hardware y fecha declarados.
