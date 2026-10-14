# car_pickup friction log

Built with the shared car builder; the family-wide snags (FLOAT COLOR_0 rewrite, bake material
names, n-gon tangents, LOD2 floor) are logged in `assets-source/kit/car_sedan/friction.md`.

1. **LOD2 over 15 %.** The bed walls and tailgate kept as full boxes at LOD2 put it at 198 tris
   (17.8 %). LOD2 now keeps only their outer and top faces and one fewer body station: 162 tris
   (14.6 %). No skill change (`gates.md` LOD floor). Lost about 3 minutes.
