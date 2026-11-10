# CLAUDE.md — Principios de trabajo para agentes

Complementa `AGENTS.md`, que sigue siendo el contrato de desarrollo. Estas reglas salen de críticas explícitas del usuario y valen para todas las sesiones.

## 1. La app es genérica: nada de sesgos
- Lakehouse RAG debe funcionar en cualquier entorno, con los documentos propios de cada uno. No existen «los documentos del usuario».
- Ninguna corrección puede depender de un documento, un fixture o una pregunta concretos. Debe ser una regla general, probada con datos distintos de los que revelaron el fallo.
- **Arreglar lo que mostró un benchmark diseñado por nosotros es un sesgo importante, no sutil.** Un 100 % en nuestras pruebas significa «funciona en los casos conocidos», no «funciona en general». Dilo siempre así.
- Las mediciones deben cubrir variedad de dominio, idioma, estructura y tamaño de documento.

## 2. Evaluación con set de prueba aislado
- La calidad real se mide con un set oculto (held-out), escrito por una sesión exclusiva que no afina la app.
- Las sesiones que corrigen la app no leen ese set. Si se usa para afinar, deja de valer y hay que escribir otro.
- Las respuestas correctas llevan página y extracto literal comprobados por script contra el documento.

## 3. Informar: lo importante primero
- **Los errores de respuesta son la noticia.** Encabezan el informe, con cuántos, cuáles y por qué. Nunca se mencionan de pasada ni detrás de métricas secundarias como reintentos o latencia.
- Distingue siempre «entregada» de «correcta».
- Separa lo medido de lo supuesto. No presentes una inferencia como hecho.
- Si una prueba se quedó corta o falta información para que sea útil (datos poco realistas, saturación al 100 %), dilo y propón cómo mejorarla **antes** de que el usuario lo pregunte.

## 4. Proporción
- Si el usuario pide medir o mirar algo, haz una herramienta sencilla: medir y proponer. No montes un proyecto alrededor.
- Prioriza lo que no gasta Claude: volver a puntuar respuestas guardadas antes que nuevas ejecuciones.
- Las mediciones de tiempo pueden estar afectadas por la carga del ordenador del usuario. Anótalo y no saques conclusiones de una sola ejecución.

## 5. Laboratorio
- Es un laboratorio de aprendizaje sin uso comercial. Los documentos públicos se usan para pruebas sin hacer hincapié en licencias.
- Sigue vigente: no versionar texto de documentos reales ni evidencia con datos reales (`docs/validacion/` está ignorado).
