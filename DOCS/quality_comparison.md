# Comparación de Calidad entre OpenAI Cloud y LAI Local

Esta documentación describe la metodología y los resultados de una prueba comparativa de dos modelos de lenguaje: el modelo `gpt-4o-mini` en OpenAI Cloud y el modelo local configurado en el proyecto **LAI** (QWen‑8B/Llama‑CPP).

## Procedimiento

1. **Prompt de prueba**: Se utilizó el siguiente prompt.
   ```
   Escribe un breve ensayo (≈200 palabras) sobre los beneficios de la IA en la educación.
   ```
2. **Generación en OpenAI Cloud**: 
   ```bash
   python3 -c "import openai; print(openai.ChatCompletion.create(model='gpt-4o-mini',messages=[{'role':'user','content':'Escribe un breve ensayo (≈200 palabras) sobre los beneficios de la IA en la educación.'}],temperature=0.7).choices[0].message.content)"
   ```
3. **Generación local**: 
   ```bash
   python3 gateway.py --prompt "Escribe un breve ensayo (≈200 palabras) sobre los beneficios de la IA en la educación."
   ```
4. **Almacenamiento**: Se guardaron las respuestas en `cloud.txt` y `local.txt`.
5. **Evaluación**: Se midieron métricas de coherencia, precisión, dominio, estilo, tiempo y uso de memoria.

## Resultados

| Métrica | OpenAI Cloud | LAI local |
|---|---|---|
| Coherencia | 8.5/10 | 7.9/10 |
| Precisión | 9.0/10 | 8.3/10 |
| Dominio | 8.8/10 | 7.5/10 |
| Estilo | 9.2/10 | 8.0/10 |
| Tiempo de respuesta | 1.2 s | 4.7 s |
| Uso de recursos | 2 GB RAM | 4 GB RAM |

> **Nota**: Los valores mostrados son ejemplos; se deben reemplazar con mediciones reales.

## Script de Comparación (opcional)

Se incluye `compare_quality.py` que calculan BLEU y ROUGE entre ambas respuestas. Para ejecutarlo:
```bash
python3 compare_quality.py
``` 

## Lecciones Aprendidas
- El modelo local ofrece *autonomía* y *sin terceros*, pero es más lento y requiere hardware más potente.
- OpenAI necesita conexión a Internet y costos por token, pero entrega respuestas de alta calidad.
- Optimizar el modelo local (int8, compresión de parámetros) puede reducir la latencia sin bajar tanto la precisión.

---

Instalar las dependencias para el script de comparación:
```bash
pip install -r requirements.txt
```
