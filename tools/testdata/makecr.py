import numpy as np, pydicom, os, sys, shutil, glob
from pydicom.dataset import Dataset, FileMetaDataset
from pydicom.sequence import Sequence
from pydicom.uid import ExplicitVRLittleEndian, generate_uid
out=sys.argv[1]; os.makedirs(out+"/cr", exist_ok=True)
R,C=400,320
yy,xx=np.mgrid[0:R,0:C]
# MONOCHROME1: low value = white. Body region low values (bright), air high (dark).
img=np.full((R,C),3800.0)
body=((xx-C/2)/140)**2+((yy-R/2)/180)**2<1; img[body]=1500
for sx in (-60,60):
    lung=((xx-C/2-sx)/45)**2+((yy-R/2+20)/110)**2<1; img[lung]=2800
img[(abs(xx-C/2)<12)&body]=600   # spine: brightest
img+=np.random.default_rng(0).normal(0,15,img.shape)
raw=np.clip(img,0,4095).astype(np.uint16)
m=FileMetaDataset(); m.MediaStorageSOPClassUID="1.2.840.10008.5.1.4.1.1.1"; m.MediaStorageSOPInstanceUID=generate_uid(); m.TransferSyntaxUID=ExplicitVRLittleEndian
d=Dataset(); d.file_meta=m; d.SOPClassUID=m.MediaStorageSOPClassUID; d.SOPInstanceUID=m.MediaStorageSOPInstanceUID
d.PatientName="PHANTOM^CHEST CR"; d.PatientID="RD0002"; d.StudyInstanceUID=generate_uid(); d.SeriesInstanceUID=generate_uid()
d.StudyDate="20261113"; d.Modality="CR"; d.SeriesNumber=1; d.InstanceNumber=1; d.SeriesDescription="PA chest VOI LUT"
d.Rows=R; d.Columns=C; d.SamplesPerPixel=1; d.PhotometricInterpretation="MONOCHROME1"
d.BitsAllocated=16; d.BitsStored=12; d.HighBit=11; d.PixelRepresentation=0; d.PixelSpacing=[0.5,0.5]
# Window in the file describes the INPUT; Horos must ignore it once the LUT is applied.
d.WindowCenter=2048; d.WindowWidth=4096
# VOI LUT: sigmoid-ish curve from stored 0..4095 to output 0..1023 (10-bit)
x=np.arange(4096); lut=(1023/(1+np.exp(-(x-2200)/350))).astype(np.uint16)
item=Dataset(); item.LUTDescriptor=[4096,0,10]; item.LUTExplanation="SIGMOID"
item.add_new(0x00283006,"US",lut.tolist())
d.VOILUTSequence=Sequence([item])
d.PixelData=raw.tobytes()
d.save_as(out+"/cr/CR0001", enforce_file_format=True)
# CT copy whose first/last 10 slices store a lung window
dst=out+"/ct_edges"; shutil.rmtree(dst, ignore_errors=True); shutil.copytree(out+"/phantom_ct", dst)
n=0
for f in glob.glob(dst+"/*"):
    ds=pydicom.dcmread(f)
    ds.SeriesInstanceUID="1.2.826.0.1.3680043.8.498.1"; ds.SeriesNumber=3; ds.SeriesDescription="Axial edges lung window"
    if ds.InstanceNumber<=10 or ds.InstanceNumber>70:
        ds.WindowCenter=-600; ds.WindowWidth=1500; n+=1
    ds.save_as(f)
print("cr ok; ct edge slices relabelled:", n)
