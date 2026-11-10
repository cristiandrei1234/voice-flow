import os, sys, onnx
import nemo.collections.asr as nemo_asr
from onnxruntime.quantization import quantize_dynamic, QuantType

# Usage: python export-parakeet-ro.py <tdt|ctc> <path/to/SpeD-ParakeetRo_110M_TDT-CTC.nemo> <output-dir>
# Needs: nemo_toolkit[asr], onnx, onnxruntime. The .nemo file comes from
# https://huggingface.co/gabrielpirlo/SpeD_ParakeetRo_110M_TDT-CTC
if len(sys.argv) != 4 or sys.argv[1] not in ("tdt", "ctc"):
    sys.exit(__doc__ or "usage: export-parakeet-ro.py <tdt|ctc> <model.nemo> <output-dir>")
SRC = sys.argv[2]
OUT = sys.argv[3]
which = sys.argv[1]

m = nemo_asr.models.ASRModel.restore_from(SRC, map_location="cpu")
m.eval()
vocab = m.tokenizer.vocab_size

def add_meta(path, meta):
    mo = onnx.load(path)
    while len(mo.metadata_props): mo.metadata_props.pop()
    for k, v in meta.items():
        p = mo.metadata_props.add(); p.key = k; p.value = str(v)
    onnx.save(mo, path)

def write_tokens(d):
    with open(f"{d}/tokens.txt", "w", encoding="utf-8") as f:
        for i in range(vocab):
            f.write(f"{m.tokenizer.ids_to_tokens([i])[0]} {i}\n")
        f.write(f"<blk> {vocab}\n")

common = dict(vocab_size=vocab, normalize_type="per_feature", subsampling_factor=8,
              model_author="SpeD / Gabriel Pirlogeanu",
              url="https://huggingface.co/gabrielpirlo/SpeD_ParakeetRo_110M_TDT-CTC",
              comment="Parakeet Ro 110M hybrid TDT-CTC, exported for sherpa-onnx")

if which == "ctc":
    d = f"{OUT}/parakeet-ro-ctc"; os.makedirs(d, exist_ok=True)
    m.set_export_config({"decoder_type": "ctc"})
    m.export(f"{d}/model.onnx")
    add_meta(f"{d}/model.onnx", {**common, "model_type": "EncDecHybridRNNTCTCBPEModel"})
    quantize_dynamic(f"{d}/model.onnx", f"{d}/model.int8.onnx", weight_type=QuantType.QUInt8)
    write_tokens(d)
else:
    d = f"{OUT}/parakeet-ro-tdt"; os.makedirs(d, exist_ok=True)
    m.set_export_config({"decoder_type": "rnnt"})
    m.encoder.export(f"{d}/encoder.onnx")
    m.decoder.export(f"{d}/decoder.onnx")
    m.joint.export(f"{d}/joiner.onnx")
    meta = {**common, "model_type": "EncDecRNNTBPEModel",
            "url": common["url"] + "?tdt",  # sherpa-onnx detects TDT by "tdt" in the url
            "pred_rnn_layers": m.cfg.decoder.prednet.pred_rnn_layers,
            "pred_hidden": m.cfg.decoder.prednet.pred_hidden, "feat_dim": 80}
    add_meta(f"{d}/encoder.onnx", meta)
    for n in ["encoder", "decoder", "joiner"]:
        quantize_dynamic(f"{d}/{n}.onnx", f"{d}/{n}.int8.onnx", weight_type=QuantType.QUInt8)
    write_tokens(d)
print("OK", d, os.listdir(d))
